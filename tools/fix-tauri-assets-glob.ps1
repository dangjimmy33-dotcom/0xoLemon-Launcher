param(
  [string]$ProjectRoot = "E:\007Launcher\src-tauri"
)

$anchorDir = Join-Path $ProjectRoot "assets\_keep"
$anchorFile = Join-Path $anchorDir "keep.txt"
New-Item -ItemType Directory -Force $anchorDir | Out-Null
Set-Content -Encoding UTF8 -Path $anchorFile -Value "0xoLemon Tauri resource anchor. Do not delete while tauri.conf.json uses assets/**/*"
Write-Host "OK: created $anchorFile"
