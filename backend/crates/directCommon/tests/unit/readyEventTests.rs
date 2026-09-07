#![allow(non_snake_case)]
use super::*;

// 等价 Windows 路径必须得到同名事件，进程或模块路径变化必须产生不同身份。
#[test]
fn readinessIsScopedToProcessAndModule() {
    let module = Path::new(r"D:\Fixture\cphook.dll");
    let event = event_name(42, module);
    assert!(event.starts_with("Local\\ObservationHookReady6-42-"));
    assert_eq!(
        event,
        event_name(42, Path::new(r"\\?\d:\fixture\cphook.dll"))
    );
    assert_ne!(event, event_name(43, module));
    assert_ne!(event, event_name(42, Path::new(r"D:\Other\cphook.dll")));
}
