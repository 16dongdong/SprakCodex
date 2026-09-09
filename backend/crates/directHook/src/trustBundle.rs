//! 额外信任只包含公开 CERTIFICATE；不可变文件保留到使用进程退出，不修改原 CA 文件或系统证书库。
use base64::{engine::general_purpose::STANDARD, Engine};
use rustls_pki_types::{pem::PemObject, CertificateDer};
use std::{
    collections::{btree_map::Entry, BTreeMap},
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
use windows::Win32::{
    Foundation::GENERIC_WRITE,
    Storage::FileSystem::{DELETE, FILE_FLAG_DELETE_ON_CLOSE, FILE_SHARE_DELETE, FILE_SHARE_READ},
};

const maxCertificateBytes: u64 = 1024 * 1024;
// 有序内容索引不依赖线程随机种子；已返回路径在进程寿命内保持有效，多次宿主重启仍复用相同证书。
#[derive(Default)]
pub(super) struct TrustBundles {
    versions: BTreeMap<Vec<u8>, TrustBundle>,
}

// FILE_FLAG_DELETE_ON_CLOSE 在硬退出时也由内核清理；句柄不可继承，不把公开文件寿命绑到 Manager。
struct TrustBundle {
    widePath: Vec<u16>,
    _file: File,
}

impl TrustBundles {
    // 仅在 TLS 配置读取时合并原自定义根和观测根；解析或写入失败返回错误，不忽略原有证书错误。
    pub(super) fn resolve(
        &mut self,
        observation: &Path,
        original: Option<&Path>,
    ) -> Result<&[u16], &'static str> {
        let mut encoded = Vec::new();
        if let Some(original) = original {
            appendCertificates(original, &mut encoded)?;
        }
        appendCertificates(observation, &mut encoded)?;
        let bundle = match self.versions.entry(encoded) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let bundle = TrustBundle::create(observation, entry.key())?;
                entry.insert(bundle)
            }
        };
        Ok(&bundle.widePath)
    }
}

impl TrustBundle {
    // 在已经配置的 CA 目录独占创建公开文件；不覆盖旧文件，不接受超过 Win32 环境变量长度的路径。
    fn create(observation: &Path, encoded: &[u8]) -> Result<Self, &'static str> {
        static sequence: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "读取证书文件时钟失败")?
            .as_nanos();
        let name = format!(
            "observationTrust-{}-{nonce}-{}.pem",
            std::process::id(),
            sequence.fetch_add(1, Ordering::Relaxed)
        );
        let path = observation
            .parent()
            .ok_or("公开证书路径缺少父目录")?
            .join(name);
        let mut widePath: Vec<u16> = path.as_os_str().encode_wide().collect();
        if widePath.len() >= 32767 || widePath.contains(&0) {
            return Err("公开证书路径长度或编码无效");
        }
        widePath.push(0);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .access_mode(GENERIC_WRITE.0 | DELETE.0)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_DELETE.0)
            .custom_flags(FILE_FLAG_DELETE_ON_CLOSE.0)
            .open(path)
            .map_err(|_| "创建进程公开证书失败")?;
        file.write_all(encoded)
            .map_err(|_| "写入进程公开证书失败")?;
        file.sync_all().map_err(|_| "提交进程公开证书失败")?;
        Ok(Self {
            widePath,
            _file: file,
        })
    }
}

// 标准 PEM 解析器只提取公开证书，再规范化编码；私钥和其他 PEM 内容绝不复制到生成文件。
fn appendCertificates(path: &Path, output: &mut Vec<u8>) -> Result<(), &'static str> {
    let mut source = Vec::new();
    File::open(path)
        .map_err(|_| "打开公开证书失败")?
        .take(maxCertificateBytes + 1)
        .read_to_end(&mut source)
        .map_err(|_| "读取公开证书失败")?;
    if source.len() as u64 > maxCertificateBytes {
        return Err("公开证书文件超过大小上限");
    }
    let mut count = 0;
    for certificate in CertificateDer::pem_slice_iter(&source) {
        let certificate = certificate.map_err(|_| "公开证书 PEM 格式错误")?;
        output.extend_from_slice(b"-----BEGIN CERTIFICATE-----\n");
        for line in STANDARD.encode(certificate.as_ref()).as_bytes().chunks(64) {
            output.extend_from_slice(line);
            output.push(b'\n');
        }
        output.extend_from_slice(b"-----END CERTIFICATE-----\n");
        count += 1;
    }
    if count == 0 {
        return Err("文件没有公开证书");
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/trustBundleTests.rs"]
mod tests;
