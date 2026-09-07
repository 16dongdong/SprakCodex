[CmdletBinding()]
param(
  [string]$Target = ""
)

$ErrorActionPreference = "Stop"

$backendRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
# 使用脚本位置定位工作区，调用方的当前目录不影响构建清单。
$argsList = @("build", "--manifest-path", (Join-Path $backendRoot "Cargo.toml"), "--release", "--locked")
if ($Target.Trim().Length -gt 0) {
  $argsList += @("--target", $Target)
}

$argsList += @(
  "-p", "codexmanager-service",
  "-p", "codexmanager-web",
  "-p", "codexmanager-start",
  "--features", "codexmanager-web/embedded-ui"
)

cargo @argsList
if ($LASTEXITCODE -ne 0) {
  exit $LASTEXITCODE
}
