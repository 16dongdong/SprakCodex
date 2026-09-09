use super::*;

type Probe = extern "system" fn(i32) -> i32;
static originalSlot: DetourSlot = DetourSlot::new();

// 独立测试函数不内联，确保调用经过真实机器入口；不挂接任何系统 API。
#[inline(never)]
extern "system" fn original(value: i32) -> i32 {
    std::hint::black_box(value) + 7
}

// 模拟生产回调首次进入就读取原调用槽，检验发布顺序而非只检验 enable 返回值。
extern "system" fn callback(value: i32) -> i32 {
    let activity = CallbackActivity::enter();
    let original: Probe = originalSlot.original();
    if activity.isUnloading() {
        return original(value);
    }
    original(value) + 30
}

// 真实 trampoline 必须在启用时可调用，disable 后恢复原入口，release 后不保留可执行页所有权。
#[test]
fn publishedTrampolineIsAvailableToFirstCallback() {
    let call = std::hint::black_box(original as Probe);
    assert_eq!(call(8), 15);
    unsafe {
        originalSlot
            .install(original as *const (), callback as *const ())
            .unwrap();
    }
    assert_eq!(call(8), 45);
    let originalCall: Probe = originalSlot.original();
    assert_eq!(originalCall(8), 15);
    originalSlot.disable().unwrap();
    assert_eq!(call(8), 15);
    originalSlot.release().unwrap();
    assert!(!originalSlot.isInstalled());
}

// 网络模块不应重新引入原工程的画像、系统代理重写或强制结束子进程逻辑。
#[test]
fn runtimeContainsOnlyObservationHooks() {
    let source = include_str!("../../src/windowsRuntime.rs");
    for unrelated in [
        "WinHttpOpen",
        "TerminateProcess",
        "NtOpenKey",
        "GetTimeZoneInformation",
        "GetUserDefaultLocaleName",
        "CreateProcessW",
    ] {
        assert!(!source.contains(unrelated), "观测模块不应调用 {unrelated}");
    }
}
