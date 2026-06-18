param(
  [string]$ProjectRoot = (Get-Location).Path
)

$ErrorActionPreference = 'Stop'
$job = Join-Path $ProjectRoot 'src-tauri\src\job.rs'
$dir = Split-Path -Parent $job
$backup = Get-ChildItem $dir -Filter 'job.rs.bak_split_*' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $backup) { throw 'Không tìm thấy backup job.rs.bak_split_*' }
Copy-Item $backup.FullName $job -Force
Write-Host "Restored $($backup.FullName) -> $job"
