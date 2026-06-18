param(
  [string]$ProjectRoot = (Get-Location).Path
)

$ErrorActionPreference = 'Stop'
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$py = Get-Command python -ErrorAction SilentlyContinue
if (-not $py) { $py = Get-Command py -ErrorAction SilentlyContinue }
if (-not $py) { throw 'Không tìm thấy python/py trong PATH. Cài Python hoặc chạy từ môi trường có Python.' }

& $py.Source (Join-Path $scriptDir 'split_backend_job.py') --project-root $ProjectRoot
