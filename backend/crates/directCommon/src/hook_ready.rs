//! cphook 与 Host 共享的就绪同步命名。

/// 生成当前进程专属的就绪事件名，避免多个目标进程共享初始化状态。
pub fn event_name(pid: u32) -> String {
    format!("Local\\CproxyHookReady-{pid}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn event_name_is_local_and_pid_scoped() {
        assert_eq!(super::event_name(42), "Local\\CproxyHookReady-42");
    }
}
