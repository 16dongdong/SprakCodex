use super::*;

type Probe = extern "system" fn(i32) -> i32;
static originalSlot: OnceLock<GenericDetour<Probe>> = OnceLock::new();

// 独立测试函数不内联，确保调用经过真实机器入口；不挂接任何系统 API。
#[inline(never)]
extern "system" fn original(value: i32) -> i32 {
    std::hint::black_box(value) + 7
}

// 模拟生产回调首次进入就读取原调用槽，检验发布顺序而非只检验 enable 返回值。
extern "system" fn callback(value: i32) -> i32 {
    originalSlot
        .get()
        .expect("回调进入时原调用槽未发布")
        .call(value)
        + 30
}

// 使用真实 retour trampoline 验证启用后的调用、原函数调用以及停用后还原。
#[test]
fn publishedTrampolineIsAvailableToFirstCallback() {
    let call = std::hint::black_box(original as Probe);
    assert_eq!(call(8), 15);
    unsafe {
        let detour = GenericDetour::new(original as Probe, callback as Probe).unwrap();
        activateDetour(&originalSlot, detour).unwrap();
        let observed = call(8);
        let originalResult = originalSlot.get().unwrap().call(8);
        originalSlot.get().unwrap().disable().unwrap();
        assert_eq!(observed, 45);
        assert_eq!(originalResult, 15);
        assert_eq!(call(8), 15);
    }
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
