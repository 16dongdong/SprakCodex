//! 加载生命周期专用 DLL：显式初始化入口只发布加载／就绪事件，可注入延迟且不安装网络回调。
#![allow(non_snake_case, non_upper_case_globals)]
#[cfg(windows)]
mod fixture {
    use std::ffi::c_void;
    use windows::Win32::{
        Foundation::{BOOL, HMODULE},
        System::Threading::{CreateEventW, SetEvent},
    };

    const maxDelayMs: u64 = 15_000;

    // 手工映射器在 CRT 初始化后同步调用；延迟只用于验证宿主的超时所有权和并发调度。
    #[no_mangle]
    pub unsafe extern "system" fn observationInitialize(stage: *mut u32) -> u32 {
        if !stage.is_null() {
            stage.write_volatile(100);
        }
        let delay = std::env::var("OBSERVATION_FIXTURE_LOAD_DELAY_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0)
            .min(maxDelayMs);
        std::thread::sleep(std::time::Duration::from_millis(delay));

        let loaded = windows::core::HSTRING::from(cpcommon::hook_ready::loaded_event_name(
            std::process::id(),
            cpcommon::relayContract::deploymentIdentity,
        ));
        if CreateEventW(None, true, true, &loaded).is_err() {
            return 0;
        }
        if std::env::var("OBSERVATION_FIXTURE_SIGNAL_READY").as_deref() != Ok("true") {
            return 1;
        }
        let ready = windows::core::HSTRING::from(cpcommon::hook_ready::event_name(
            std::process::id(),
            cpcommon::relayContract::deploymentIdentity,
        ));
        match CreateEventW(None, true, false, &ready) {
            Ok(event) if SetEvent(event).is_ok() => 1,
            _ => 0,
        }
    }

    // CRT 入口只执行标准模块初始化，测试行为全部集中在显式入口，匹配生产内存部署协议。
    #[no_mangle]
    pub unsafe extern "system" fn DllMain(_: HMODULE, _: u32, _: *mut c_void) -> BOOL {
        BOOL(1)
    }
}
