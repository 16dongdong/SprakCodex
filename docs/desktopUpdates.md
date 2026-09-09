# 桌面更新协议

- 发布仓库：`16dongdong/SprakCodex`。
- 程序版本：`0.6.1`；正式标签必须使用 `v0.6.1`，后续版本严格递增。
- Windows 附件：`SprakCodex_0.6.1_x64-setup.exe`。`updateAgent.exe` 随安装包部署，不常驻。
- 设置中的 `silentUpdate` 默认关闭，持久化为 `app.silent_update`。开启会启用自动检查；关闭自动检查也关闭静默更新。
- 静默模式发现新版后下载，不唤起主窗口或显示更新通知；有未保存设置或在途请求时等待。
- 主程序原子进入排空后复制更新器到应用更新缓存，传递本地作业并等待就绪。更新器持有原进程句柄，原进程正常退出前不运行安装包。
- 完整性使用固定仓库 HTTPS API 返回的发布附件 SHA-256；缺少摘要、包名不匹配或版本未递增均停止更新。这不是 Authenticode 或离线签名清单验证。
- 更新器备份应用 EXE、观测 DLL 和自身文件。安装失败或新进程提前退出时恢复这些二进制并启动旧程序；用户数据库不参与回滚，安装器注册表副作用不在二进制恢复范围内。
- 启动验证目前为十秒进程存活检查。作业目录的 `job.result.log` 记录结果；需要系统提权的安装仍受 Windows 权限机制管理。
- 0.6.0 用户先安装本版以获得独立更新器；后续正式版本将使用新流程。

## 验证

```powershell
cargo test --manifest-path frontend/updateAgent/Cargo.toml
cargo test --manifest-path frontend/src-tauri/Cargo.toml --lib commands::updater
cargo test --manifest-path backend/Cargo.toml -p codexmanager-service --lib updateActivity
pnpm -C frontend run test:runtime
```

交付安装包到 `input/` 后，按项目约定清理构建缓存，并额外执行：

```powershell
cargo clean --manifest-path frontend/updateAgent/Cargo.toml
```

## 0.6.3 程序名称迁移

从本版开始主程序为 `SprakCodex.exe`。发布附件统一命名为 `SprakCodex-0.6.3-windows-x64-setup.exe`（后续按版本递增）。旧 0.6.1/0.6.2 更新器只接受旧附件名并按旧程序名重启，因此不会自动安装新命名附件；这些版本需要手动升级一次，之后恢复正常自动更新。

DLL 仍采用系统正常加载流程，不包含绕过安全产品的内存注入实现。安装时如提示 DLL 写入失败，应检查目标设备上的文件占用、安装目录权限与安全产品拦截日志；写入失败本身不证明病毒误报。
