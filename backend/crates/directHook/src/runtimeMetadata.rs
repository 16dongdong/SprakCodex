//! 仅发布当前进程选择的 CLI 数据目录；不读取 auth.json，不修改环境变量、身份或配置。
use std::{
    ffi::OsString, os::windows::ffi::OsStringExt, os::windows::io::OwnedHandle, path::PathBuf,
    sync::OnceLock,
};
use windows::Win32::Foundation::{
    GetLastError, SetLastError, ERROR_ENVVAR_NOT_FOUND, ERROR_SUCCESS,
};
use windows::{
    core::{w, PCWSTR},
    Win32::System::Environment::GetEnvironmentVariableW,
};

static mapping: OnceLock<OwnedHandle> = OnceLock::new();
const maxEnvironmentUnits: usize = 32768;

// loader lock 外先发布元数据再置位模块就绪；句柄保留到目标进程结束，宿主重启可重新读取。
pub(super) fn publish() -> Result<(), &'static str> {
    let home = match variable(w!("CODEX_HOME"))? {
        Some(home) => PathBuf::from(home),
        None => PathBuf::from(variable(w!("USERPROFILE"))?.ok_or("目标进程缺少 CLI home")?)
            .join(".codex"),
    };
    mapping
        .set(cpcommon::runtimeHome::publish(
            cpcommon::relayContract::deploymentIdentity,
            &home,
        )?)
        .map_err(|_| "运行目录已发布")
}

// 只查询固定的公开目录变量；非 CA 名称由已安装读取入口原样转发，长度和编码不做猜测。
fn variable(name: PCWSTR) -> Result<Option<OsString>, &'static str> {
    let mut units = vec![0u16; maxEnvironmentUnits];
    unsafe {
        SetLastError(ERROR_SUCCESS);
    }
    let length = unsafe { GetEnvironmentVariableW(name, Some(&mut units)) } as usize;
    if length == 0
        && !matches!(
            unsafe { GetLastError() },
            ERROR_SUCCESS | ERROR_ENVVAR_NOT_FOUND
        )
    {
        return Err("读取目标目录环境变量失败");
    }
    if length >= units.len() {
        return Err("目标目录环境变量过长");
    }
    Ok((length != 0).then(|| OsString::from_wide(&units[..length])))
}
