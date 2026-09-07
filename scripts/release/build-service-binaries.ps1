[CmdletBinding()]
param(
  [string]$Target = ""
)

$ErrorActionPreference = "Stop"

$argsList = @("build", "--release", "--locked")
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
