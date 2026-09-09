#![allow(non_snake_case)]
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

// 作业只含本机安装参数与公开摘要；不接受任意命令、下载地址或凭据。
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateJob {
    pub parentPid: u32,
    pub installer: std::path::PathBuf,
    pub targetExe: std::path::PathBuf,
    pub expectedSha256: String,
    pub currentVersion: String,
    pub targetVersion: String,
    pub readyPath: std::path::PathBuf,
    pub pendingPath: std::path::PathBuf,
}

// 流式计算 SHA-256，不把安装包读入内存；文件读取错误保持原始失败原因。
pub fn fileDigest(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

// 作业必须指向真实安装与精确版本安装包，且版本递增；摘要不符时主程序继续运行。
pub fn validateJob(job: &UpdateJob) -> Result<(), String> {
    let current = semver::Version::parse(&job.currentVersion).map_err(|e| e.to_string())?;
    let target = semver::Version::parse(&job.targetVersion).map_err(|e| e.to_string())?;
    if target <= current {
        return Err("目标版本没有递增".into());
    }
    if !job.targetExe.is_absolute() || !job.targetExe.is_file() || !job.installer.is_absolute() {
        return Err("更新路径无效".into());
    }
    let expectedName = format!("SprakCodex_{}_x64-setup.exe", target);
    if job.installer.file_name().and_then(|n| n.to_str()) != Some(expectedName.as_str()) {
        return Err("安装包名称与目标版本不符".into());
    }
    if job.expectedSha256.len() != 64
        || !job.expectedSha256.bytes().all(|b| b.is_ascii_hexdigit())
        || fileDigest(&job.installer)? != job.expectedSha256.to_lowercase()
    {
        return Err("安装包完整性校验失败".into());
    }
    Ok(())
}
