use super::*;

// 文件名匹配忽略 Windows 大小写；ChatGPT 桌面壳及 Codex 请求子进程都应进入候选集。
#[test]
fn selectionRequiresTargetExecutableName() {
    for name in [
        "chatgpt.exe",
        "ChatGPT.exe",
        "codex.exe",
        "Codex.exe",
        "codex-app.exe",
    ] {
        assert!(isTargetExecutable(Path::new(name)));
    }
    for name in [
        "server.exe",
        "node.exe",
        "app-server",
        "my-chatgpt.exe",
        "chatgpt.exe.backup",
        "my-codex.exe",
        "codex.exe.backup",
    ] {
        assert!(!isTargetExecutable(Path::new(name)));
    }
}
