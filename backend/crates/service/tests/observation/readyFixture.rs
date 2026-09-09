//! 加载生命周期专用 DLL：显式初始化入口只发布加载／就绪事件，可注入延迟且不安装网络回调。
#![allow(non_snake_case, non_upper_case_globals)]
#[cfg(windows)]
mod fixture {
    use std::{
        ffi::c_void,
        sync::atomic::{AtomicIsize, Ordering},
    };
    use windows::Win32::{
        Foundation::{CloseHandle, BOOL, HANDLE, HMODULE},
        System::Threading::{CreateEventW, SetEvent},
    };

    const maxDelayMs: u64 = 15_000;
    static loadedEvent: AtomicIsize = AtomicIsize::new(0);
    static readyEvent: AtomicIsize = AtomicIsize::new(0);

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
        let Ok(loaded) = CreateEventW(None, true, true, &loaded) else {
            return 0;
        };
        loadedEvent.store(loaded.0 as isize, Ordering::Release);
        if std::env::var("OBSERVATION_FIXTURE_SIGNAL_READY").as_deref() != Ok("true") {
            return 1;
        }
        let ready = windows::core::HSTRING::from(cpcommon::hook_ready::event_name(
            std::process::id(),
            cpcommon::relayContract::deploymentIdentity,
        ));
        match CreateEventW(None, true, false, &ready) {
            Ok(event) if SetEvent(event).is_ok() => {
                readyEvent.store(event.0 as isize, Ordering::Release);
                1
            }
            _ => 0,
        }
    }

    // CRT 入口只执行标准模块初始化，测试行为全部集中在显式入口，匹配生产内存部署协议。
    #[no_mangle]
    pub unsafe extern "system" fn DllMain(_: HMODULE, _: u32, _: *mut c_void) -> BOOL {
        BOOL(1)
    }

    // 生命周期夹具按生产 ABI 注销异常表并执行 CRT detach，供宿主验证映像可重复释放。
    #[no_mangle]
    pub unsafe extern "system" fn observationShutdown(context: *mut c_void) -> u32 {
        if context.is_null() {
            return 0;
        }
        let context = *(context as *const cpcommon::deploymentLifecycle::UnloadContext);
        for event in [&readyEvent, &loadedEvent] {
            let raw = event.swap(0, Ordering::AcqRel);
            if raw != 0 {
                let _ = CloseHandle(HANDLE(raw as *mut _));
            }
        }
        type EntryPoint = unsafe extern "system" fn(HMODULE, u32, *mut c_void) -> BOOL;
        let entryPoint: EntryPoint = std::mem::transmute(context.entryPoint);
        let _ = entryPoint(
            HMODULE(context.imageBase as *mut _),
            0,
            std::ptr::null_mut(),
        );
        if context.functionTable != 0
            && !windows::Win32::System::Diagnostics::Debug::RtlDeleteFunctionTable(
                context.functionTable as *const _,
            )
            .as_bool()
        {
            return 0;
        }
        1
    }
}
