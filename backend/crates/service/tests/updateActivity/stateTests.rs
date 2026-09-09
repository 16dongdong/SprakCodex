use super::ActivityState;

// 使用独立状态对象验证排空转换，不污染同进程网络回归测试的全局请求计数。
#[test]
fn drainingRequiresIdleAndRejectsNewWork() {
    let mut state = ActivityState::default();
    state.begin().unwrap();
    assert!(!state.drain());
    state.active = 0;
    assert!(state.drain());
    assert!(state.begin().is_err());
    assert!(!state.drain());
    state.draining = false;
    assert!(state.begin().is_ok());
}
