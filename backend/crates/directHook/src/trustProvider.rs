//! 只接入 Codex 已支持的额外 CA 读取点；原有变量值不落日志，也不修改进程环境或登录状态。
use super::{hookInstall, imp, trustBundle::TrustBundles};
use std::{
    ffi::OsString,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
    sync::Mutex,
};
use windows::{
    core::{s, w},
    Win32::{
        Foundation::{GetLastError, SetLastError},
        System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
    },
};

type GetVariable = unsafe extern "system" fn(*const u16, *mut u16, u32) -> u32;
static originalVariable: hookInstall::DetourSlot = hookInstall::DetourSlot::new();
static trustState: Mutex<TrustState> = Mutex::new(TrustState {
    bundles: None,
    provided: None,
    lastFailure: None,
});
const caVariable: &[u8] = b"CODEX_CA_CERTIFICATE";
const maxVariableChars: usize = 32768;

// 只有已完整返回证书路径才标记提供；这不等价于既有 TLS 客户端已经重建，旧连接仍独立验收。
#[derive(Default)]
struct TrustState {
    bundles: Option<TrustBundles>,
    provided: Option<PathBuf>,
    lastFailure: Option<&'static str>,
}

// 初始化线程安装读取入口；失败向 worker 返回，让网络改连保持关闭，不留下半启用 TLS 接入。
pub(super) unsafe fn install() -> Result<(), String> {
    let module = GetModuleHandleW(w!("kernel32.dll")).map_err(|_| "读取环境变量模块失败")?;
    let address =
        GetProcAddress(module, s!("GetEnvironmentVariableW")).ok_or("读取环境变量入口失败")?;
    originalVariable.install(address as *const (), readVariable as *const ())
}

// 新连接在额外证书路径尚未提供时保持原路由，避免注入先于 TLS 配置读取时提前发送观测证书。
pub(super) fn providedFor(path: &Path) -> bool {
    trustState
        .lock()
        .is_ok_and(|state| state.provided.as_deref() == Some(path))
}

// Win32 回调只比较固定变量名；其他变量直接执行 trampoline，缓冲区尺寸和 LastError 语义保持不变。
unsafe extern "system" fn readVariable(name: *const u16, buffer: *mut u16, capacity: u32) -> u32 {
    let activity = hookInstall::CallbackActivity::enter();
    let original: GetVariable = originalVariable.original();
    if activity.isUnloading() {
        return original(name, buffer, capacity);
    }
    if !matchesCaVariable(name) {
        return original(name, buffer, capacity);
    }
    let lastError = GetLastError();
    if let Some(length) = provideBundle(buffer, capacity) {
        SetLastError(lastError);
        return length;
    }
    original(name, buffer, capacity)
}

// 仅读取原环境的 CA 路径，通过 trampoline 避免递归进入 Rust 的环境锁；失败不发布额外路径。
unsafe fn provideBundle(buffer: *mut u16, capacity: u32) -> Option<u32> {
    let settings = imp::relaySnapshot()?;
    let observation = settings.caCertificatePath.as_deref()?;
    let mut state = trustState.lock().ok()?;
    let selected = selectOriginalCa().and_then(|original| {
        state
            .bundles
            .get_or_insert_with(TrustBundles::default)
            .resolve(observation, original.as_deref())
    });
    match selected {
        Ok(path) => {
            let (length, copied) = copyBundlePath(path, buffer, capacity);
            if copied {
                state.provided = Some(observation.to_owned());
            }
            state.lastFailure = None;
            Some(length)
        }
        Err(error) => {
            state.provided = None;
            if state.lastFailure != Some(error) {
                imp::log(error);
                state.lastFailure = Some(error);
            }
            None
        }
    }
}

// Win32 成功长度不含 NUL，容量不足返回包含 NUL 的所需长度且不写缓冲区；布尔值仅表示已完整交付路径。
unsafe fn copyBundlePath(path: &[u16], buffer: *mut u16, capacity: u32) -> (u32, bool) {
    let length = path.len() as u32;
    if capacity < length || buffer.is_null() {
        return (length, false);
    }
    std::ptr::copy_nonoverlapping(path.as_ptr(), buffer, path.len());
    (length - 1, true)
}

// 遵循官方 CODEX_CA_CERTIFICATE 优先、SSL_CERT_FILE 次之、空字符串忽略的规则，不读取任何认证变量。
unsafe fn selectOriginalCa() -> Result<Option<PathBuf>, &'static str> {
    for key in [w!("CODEX_CA_CERTIFICATE"), w!("SSL_CERT_FILE")] {
        let original: GetVariable = originalVariable.original();
        let mut value = vec![0u16; maxVariableChars];
        let length = original(key.as_ptr(), value.as_mut_ptr(), value.len() as u32) as usize;
        if length >= value.len() {
            return Err("原公开证书环境变量过长");
        }
        if length == 0 {
            continue;
        }
        let value = OsString::from_wide(&value[..length]);
        if value.to_str().is_some() {
            return Ok(Some(PathBuf::from(value)));
        }
    }
    Ok(None)
}

// 卸载第一阶段恢复环境变量入口；回调排空后再释放 trampoline 和公开证书文件。
pub(super) fn disable() -> Result<(), String> {
    originalVariable.disable()
}

// 调用方保证没有活跃回调；清空证书缓存会关闭 DELETE_ON_CLOSE 文件句柄。
pub(super) fn release() -> Result<(), String> {
    originalVariable.release()?;
    *trustState.lock().map_err(|_| "公开证书状态锁损坏")? = TrustState::default();
    Ok(())
}

// 按 Windows 环境变量规则仅对 ASCII 名称忽略大小写，比较至固定结尾，不扫描任意长度输入。
unsafe fn matchesCaVariable(name: *const u16) -> bool {
    if name.is_null() {
        return false;
    }
    for (index, expected) in caVariable.iter().enumerate() {
        let actual = *name.add(index);
        if actual > 127 || !(actual as u8).eq_ignore_ascii_case(expected) {
            return false;
        }
    }
    *name.add(caVariable.len()) == 0
}

#[cfg(test)]
#[path = "../tests/unit/trustProviderTests.rs"]
mod tests;
