# 请求详情与现有连接接入验收

日期：2026-09-08，Windows 桌面实际安装环境。

## 已交付

- 会话 JSON 导入保留同一主体已有的刷新授权；仅有 Access Token 时，使用登录授权获取后续凭据。
- 直连观测使用紧凑开关行，和代理中转共用 `48760`。
- 日志保留时间、类型、方法、路径、账号名称／邮箱、凭据指纹、模型、推理、等级、状态、用时、首响、用量及错误。
- 详情按列表的 `traceId` 查询，分别显示请求头、请求体、响应头、响应体。完整正文使用自动删除的临时文件采集，入库前脱敏，不再只保存固定长度预览。
- `observationHook9.dll` 在宿主恢复时定向重接入仍连接已知本机 HTTP 代理的旧连接；不终止客户端进程，不改登录、系统代理或系统证书库。

## 当前客户端真实记录

验收使用正在工作的 VS Code 内 Codex，而非仅启动新的测试 CLI。记录 `317`：

| 项目 | 结果 |
| --- | --- |
| 模型／方法／状态 | `gpt-6-astra`／`GET`／`200` |
| 用时／首响 | `74520 ms`／`1994 ms` |
| 必需日志字段 | 全部存在 |
| 名称及邮箱 | 已提取并显示，报告不抄录个人信息 |
| 请求头／响应头 | `19`／`17` 个字段 |
| 请求体 | `4981` 字节，完整结构可读 |
| 响应体 | `849` 帧，`445688` 字节，包含最终完成帧 |
| 详情 RPC | 按 `traceId` 返回内容与 SQLite 记录一致 |
| 钱包扣款 | `0` |
| 原登录／配置 | 文件哈希未变化 |

同一账号及相同凭据指纹的旧网络记录已经补齐名称。没有报文的历史客户端完成事件保持“未采集”标记，不生成虚假的请求头、方法或状态。

## 自动验证

- 后端工作区：`2293` 通过、`0` 失败、`18` 忽略。
- 桌面壳：`76` 通过。
- 前端运行时：`226` 通过。
- 浏览器：四个报文区域、邮箱名称、详情查询参数和分页回归通过。
- 原生模块：`22` 通过，包含只重连选定 TCP 对端且其他连接继续传输的实 socket 测试。
- 完整报文测试覆盖超过旧上限的正文、响应增量帧、重复头、认证脱敏以及历史记录权限隔离。

## 安装与回滚

安装包：`frontend/src-tauri/target/release/bundle/nsis/CodexManager_0.6.0_x64-setup.exe`。

安装目录：`D:\CodexManager`。NSIS 安装退出码为 `0`；程序仅含预期的 `NSS` 包类型标记差异，其余字节与构建产物一致，DLL 字节一致。

回滚点：`D:\CodexManager\rollback-fullCapture9-20260908T032112Z`，包含旧程序、旧模块及当前用户 DPAPI 加密的数据库快照。旧的已映射模块保留，不强制卸载。

原生句柄接入使用 Windows [PssCaptureSnapshot](https://learn.microsoft.com/en-us/windows/win32/api/processsnapshot/nf-processsnapshot-psscapturesnapshot) 和 [PssWalkSnapshot](https://learn.microsoft.com/en-us/windows/win32/api/processsnapshot/nf-processsnapshot-psswalksnapshot)，仅捕获本进程句柄，不复制地址空间或冻结线程。
