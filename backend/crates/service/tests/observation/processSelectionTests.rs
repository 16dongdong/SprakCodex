use super::*;

// 文件名匹配忽略 Windows 大小写，普通 server.exe 即使启动参数为 app-server 也不属于目标。
#[test]
fn selectionRequiresTargetExecutableName() {
    for name in ["codex.exe", "Codex.exe", "codex-app.exe"] {
        assert!(isTargetExecutable(Path::new(name)));
    }
    for name in ["server.exe", "node.exe", "app-server", "my-codex.exe", "codex.exe.backup"] {
        assert!(!isTargetExecutable(Path::new(name)));
    }
}
