//! Windows 用户范围保护观测 CA 签名密钥；密钥只以 DPAPI 密文持久化，不安装根证书，也不读取登录材料。
use rcgen::KeyPair;
use std::{
    io::{Read, Write},
    path::Path,
};
use windows::Win32::{
    Foundation::{LocalFree, HLOCAL},
    Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    },
};
use zeroize::{Zeroize, Zeroizing};

const protectedKeyName: &str = "observationAuthority.dpapi";
const keyLockName: &str = "observationAuthority.lock";
const maximumKeyBytes: u64 = 64 * 1024;

// 通过文件锁串行化同一数据目录中的首次初始化；已有密钥解密失败直接报错，不悄悄生成不同的信任身份。
pub(super) fn loadOrCreate(directory: &Path) -> Result<KeyPair, String> {
    let mut lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(keyLockName))
        .map_err(|_| "打开观测签名身份锁失败")?;
    lock.try_lock()
        .map_err(|_| "另一个进程正在初始化观测签名身份")?;
    lock.set_len(0).map_err(|_| "更新观测签名身份锁失败")?;
    write!(lock, "{}", std::process::id()).map_err(|_| "写入观测签名身份锁失败")?;
    let path = directory.join(protectedKeyName);
    match std::fs::metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximumKeyBytes {
                return Err("观测签名身份文件大小无效".into());
            }
            // 再限制实际读取长度，避免 metadata 检查后文件增长导致无界分配。
            let mut protected = Vec::new();
            std::fs::File::open(&path)
                .map_err(|_| "打开观测签名身份失败")?
                .take(maximumKeyBytes + 1)
                .read_to_end(&mut protected)
                .map_err(|_| "读取观测签名身份失败")?;
            if protected.len() as u64 > maximumKeyBytes {
                return Err("观测签名身份文件超过限额".into());
            }
            let plain = transformUserProtectedKey(&protected, false)?;
            let der = rustls::pki_types::PrivatePkcs8KeyDer::from(plain.as_slice());
            KeyPair::try_from(&der).map_err(|_| "观测签名身份不是有效密钥".into())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let signingKey = KeyPair::generate().map_err(|_| "生成观测签名身份失败")?;
            let plain = Zeroizing::new(signingKey.serialize_der());
            let protected = transformUserProtectedKey(&plain, true)?;
            super::runtimePaths::writeAtomically(&path, &protected)?;
            Ok(signingKey)
        }
        Err(_) => Err("检查观测签名身份文件失败".into()),
    }
}

// DPAPI 分配的内存由 LocalFree 释放；解密输出在释放前清零，不将明文复制到日志或磁盘。
struct LocalBlob(CRYPT_INTEGER_BLOB);
impl Drop for LocalBlob {
    // 系统分配成功时才访问长度范围，零长度或空指针不构造无效切片。
    fn drop(&mut self) {
        if !self.0.pbData.is_null() {
            unsafe {
                std::slice::from_raw_parts_mut(self.0.pbData, self.0.cbData as usize).zeroize();
                let _ = LocalFree(HLOCAL(self.0.pbData.cast()));
            }
        }
    }
}

// 只使用当前用户 DPAPI 范围，不设置机器范围，也不弹出系统提示；不同用户或损坏密文返回明确失败。
fn transformUserProtectedKey(input: &[u8], protect: bool) -> Result<Zeroizing<Vec<u8>>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: input.len().try_into().map_err(|_| "观测签名身份长度溢出")?,
        pbData: input.as_ptr() as *mut u8,
    };
    let mut output = LocalBlob(CRYPT_INTEGER_BLOB::default());
    unsafe {
        if protect {
            CryptProtectData(
                &input,
                windows::core::w!("本地观测签名身份"),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output.0,
            )
            .map_err(|_| "保护观测签名身份失败")?;
        } else {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output.0,
            )
            .map_err(|_| "观测签名身份属于其他用户或密文已损坏")?;
        }
        if output.0.pbData.is_null() || output.0.cbData == 0 {
            return Err("观测签名身份保护接口返回空内容".into());
        }
        Ok(Zeroizing::new(
            std::slice::from_raw_parts(output.0.pbData, output.0.cbData as usize).to_vec(),
        ))
    }
}

#[cfg(test)]
#[path = "../../tests/observation/authorityStoreTests.rs"]
mod tests;
