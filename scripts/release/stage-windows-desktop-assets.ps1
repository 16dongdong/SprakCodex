[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$ReleaseDir,

  [Parameter(Mandatory = $true)]
  [string]$RunnerArch,

  [string]$TauriConfigPath = "apps/src-tauri/tauri.conf.json"
)

$ErrorActionPreference = "Stop"

$bundleDir = Join-Path $ReleaseDir "bundle"
$portableDir = Join-Path $ReleaseDir "portable"
$desktopAssetsDir = Join-Path $ReleaseDir "release-assets"

if (-not (Test-Path $TauriConfigPath -PathType Leaf)) {
  throw "tauri config not found: $TauriConfigPath"
}

$version = (Get-Content $TauriConfigPath -Raw | ConvertFrom-Json).version
$arch = if ($RunnerArch -eq "ARM64") { "arm64" } else { "x64" }

New-Item -ItemType Directory -Force $portableDir | Out-Null
Remove-Item -Recurse -Force $desktopAssetsDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $desktopAssetsDir | Out-Null

$source = Join-Path $ReleaseDir "CodexManager.exe"
if (-not (Test-Path $source -PathType Leaf)) {
  throw "portable source binary not found: $source"
}

$portableBinary = Join-Path $portableDir "CodexManager-portable.exe"
Copy-Item -Force $source $portableBinary
Write-Host "portable binary: $portableBinary"

$installerSource = Get-ChildItem -Path $bundleDir -Recurse -File |
  Where-Object { $_.Name -like "*-setup.exe" -and $_.Name -notlike "*portable*" } |
  Sort-Object FullName |
  Select-Object -First 1

if (-not $installerSource) {
  throw "windows setup installer not found under: $bundleDir"
}

$installerTarget = Join-Path $desktopAssetsDir "CodexManager_${version}_${arch}-setup.exe"
Copy-Item -Force $installerSource.FullName $installerTarget
Copy-Item -Force $portableBinary (Join-Path $desktopAssetsDir "CodexManager-portable.exe")

Get-ChildItem -Path $desktopAssetsDir -File | ForEach-Object {
  Write-Host "desktop asset: $($_.FullName)"
}
