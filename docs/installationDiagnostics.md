# 安装写入失败与误报诊断

`Error opening file for writing` 只说明安装器没有写入目标文件，并不等同于病毒判定。应在发生问题的设备上区分：

1. 旧 DLL 被仍在运行的客户端加载：先结束当前工作，正常退出相关客户端与管理程序，再安装。
2. 目录权限或只读属性：核对安装目录和实际运行用户，不通过放宽全盘权限处理。
3. 安全软件隔离或拦截：保留安全软件名称、检测名称、文件 SHA-256 和事件时间，再向对应厂商提交误报复核。

以下命令仅查询，不关闭防护、不添加排除项：

```powershell
Get-FileHash -Algorithm SHA256 -LiteralPath 'D:\SprakCodex\observationHook9.dll'
Get-AuthenticodeSignature -LiteralPath 'D:\SprakCodex\observationHook9.dll'
Get-MpThreatDetection
```

当前尚未配置代码签名证书，发布文件未宣称已获得受信任签名。后续签名应使用维护者的有效代码签名证书，并对 DLL、更新器与主程序一并签名。签名和误报复核不保证所有产品立即消除告警。

本项目保留系统正常 DLL 加载方式，不使用规避防护的内存注入。截图来自另一台设备，本机的防护记录不能替代那台设备的调查结果。
