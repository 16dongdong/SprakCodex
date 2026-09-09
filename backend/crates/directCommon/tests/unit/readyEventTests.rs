#![allow(non_snake_case)]
use super::*;

// 进程或部署版本变化必须产生不同事件，加载与就绪名称也必须隔离。
#[test]
fn readinessIsScopedToProcessAndModule() {
    let event = event_name(42, "fixture-10");
    assert!(event.starts_with("Local\\ObservationHookReady11-42-"));
    assert_ne!(event, event_name(43, "fixture-10"));
    assert_ne!(event, event_name(42, "fixture-11"));
    assert_ne!(event, loaded_event_name(42, "fixture-10"));
}
