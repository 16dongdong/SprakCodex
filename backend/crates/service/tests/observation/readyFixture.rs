//! 加载生命周期专用 DLL：只发就绪事件，可显式注入加载延迟；不安装 hook、不修改网络或账户状态。
#![allow(non_snake_case, non_upper_case_globals)]
#[cfg(windows)]
mod fixture {
    use std::ffi::c_void;
    use windows::Win32::{
        Foundation::{BOOL, HMODULE},
        System::Threading::{CreateEventW, CreateThread, SetEvent, THREAD_CREATION_FLAGS},
    };

    const processAttach: u32 = 1;
    const maxDelayMs: u64 = 15_000;

    // 工作线程只创建并保留就绪事件；环境开关为 false 时故意不就绪，模拟加载后初始化失败。
    unsafe extern "system" fn signalReady(module: *mut c_void) -> u32 {
        if std::env::var("OBSERVATION_FIXTURE_SIGNAL_READY").as_deref() != Ok("true") {
            return 0;
        }
        let mut path = [0u16; 32768];
        let length =
            windows::Win32::System::LibraryLoader::GetModuleFileNameW(HMODULE(module), &mut path)
                as usize;
        if length == 0 || length == path.len() {
            return 1;
        }
        let path = std::path::PathBuf::from(String::from_utf16_lossy(&path[..length]));
        let name = windows::core::HSTRING::from(cpcommon::hook_ready::event_name(
            std::process::id(),
            &path,
        ));
        if let Ok(event) = CreateEventW(None, true, false, &name) {
            if SetEvent(event).is_err() {
                return 1;
            }
            // 命名事件必须活到目标退出，宿主可能在稍后的扫描周期打开它。
        }
        0
    }

    // 故障注入只存在于测试 DLL：可暂停 loader 回调，验证宿主超时后不提前释放远程参数。
    #[no_mangle]
    pub unsafe extern "system" fn DllMain(module: HMODULE, reason: u32, _: *mut c_void) -> BOOL {
        if reason == processAttach {
            let delay = std::env::var("OBSERVATION_FIXTURE_LOAD_DELAY_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0)
                .min(maxDelayMs);
            std::thread::sleep(std::time::Duration::from_millis(delay));
            if let Ok(thread) = CreateThread(
                None,
                0,
                Some(signalReady),
                Some(module.0),
                THREAD_CREATION_FLAGS(0),
                None,
            ) {
                let _ = windows::Win32::Foundation::CloseHandle(thread);
            }
        }
        BOOL(1)
    }
}
