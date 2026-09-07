use super::*;
use std::{os::windows::ffi::OsStringExt, path::PathBuf};

// 本模块只处理 PEM 封装，不进行 X.509 链校验；完整证书校验由真实 TLS 客户端测试覆盖。
const firstCertificate: &[u8] = b"-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----\n";
const secondCertificate: &[u8] = b"-----BEGIN CERTIFICATE-----\nBAUG\n-----END CERTIFICATE-----\n";

// 每例创建独立普通文件，析构只删除该目录内已知文件，不访问系统证书或用户 CA。
struct Fixture(PathBuf);
impl Fixture {
    // 唯一目录防止并发证书缓存测试相互覆盖。
    fn new() -> Self {
        static sequence: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "trustBundle{}-{}",
            std::process::id(),
            sequence.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory)
    }
    // 仅写调用方明确提供的 PEM 测试字节。
    fn write(&self, name: &str, encoded: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, encoded).unwrap();
        path
    }
}
impl Drop for Fixture {
    // TrustBundles 先析构关闭删除句柄，随后严格清理输入文件；共享冲突不能被忽略。
    fn drop(&mut self) {
        for entry in std::fs::read_dir(&self.0).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(&self.0).unwrap();
    }
}

// 合并后仍保留原 CA，缓存相同公开内容，旧路径在新版本发布后也保持可读。
#[test]
fn combinedBundlePreservesOriginalAndRetainsReturnedPaths() {
    let fixture = Fixture::new();
    let observation = fixture.write("observation.pem", firstCertificate);
    let original = fixture.write("original.pem", secondCertificate);
    let mut bundles = TrustBundles::default();
    let path = bundlePath(bundles.resolve(&observation, Some(&original)).unwrap());
    assert_eq!(
        path,
        bundlePath(bundles.resolve(&observation, Some(&original)).unwrap())
    );
    let mut expected = secondCertificate.to_vec();
    expected.extend_from_slice(firstCertificate);
    assert_eq!(std::fs::read(&path).unwrap(), expected);
    let changed = bundlePath(bundles.resolve(&observation, None).unwrap());
    assert_ne!(path, changed);
    assert_eq!(std::fs::read(&path).unwrap(), expected);
    assert!(OpenOptions::new().write(true).open(&path).is_err());
    drop(bundles);
    assert!(!path.exists());
    assert!(!changed.exists());
}

// 非证书 PEM 块绝不写入公开输出，空文件、坏 base64、超大文件均明确失败。
#[test]
fn privateBlocksAreExcludedAndInvalidInputIsRejected() {
    let fixture = Fixture::new();
    let mut mixed = b"-----BEGIN PRIVATE KEY-----\nBwgJ\n-----END PRIVATE KEY-----\n".to_vec();
    mixed.extend_from_slice(firstCertificate);
    let observation = fixture.write("mixed.pem", &mixed);
    let mut bundles = TrustBundles::default();
    let path = bundlePath(bundles.resolve(&observation, None).unwrap());
    assert_eq!(std::fs::read(path).unwrap(), firstCertificate);
    for invalid in [
        Vec::new(),
        b"-----BEGIN CERTIFICATE-----\n!!!\n-----END CERTIFICATE-----\n".to_vec(),
        vec![b' '; maxCertificateBytes as usize + 1],
    ] {
        let path = fixture.write("invalid.pem", &invalid);
        assert!(bundles.resolve(&path, None).is_err());
    }
}

// 回调返回的 UTF-16 路径包含 NUL，文件系统测试移除终止符再构造 Windows 路径。
fn bundlePath(wide: &[u16]) -> PathBuf {
    PathBuf::from(std::ffi::OsString::from_wide(&wide[..wide.len() - 1]))
}

// Manager 重启可能重新签发同一根身份的公开证书；超过十六次内容更新也不得停止提供新路径或删除旧路径。
#[test]
fn repeatedCertificateUpdatesKeepAllPublishedPathsValid() {
    let fixture = Fixture::new();
    let observation = fixture.0.join("observation.pem");
    let mut bundles = TrustBundles::default();
    let mut paths = Vec::new();
    for version in 0..32u8 {
        let encoded = format!(
            "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
            STANDARD.encode([version])
        );
        std::fs::write(&observation, &encoded).unwrap();
        let path = bundlePath(bundles.resolve(&observation, None).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), encoded.as_bytes());
        paths.push(path);
    }
    assert!(paths.iter().all(|path| path.is_file()));
    assert_eq!(bundles.versions.len(), 32);
    drop(bundles);
    assert!(paths.iter().all(|path| !path.exists()));
}
