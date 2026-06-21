param(
  [string]$ProjectRoot = (Get-Location).Path
)

$ErrorActionPreference = 'Stop'
$src = Join-Path $ProjectRoot 'src-tauri\src'
Write-Host 'Searching possible HuggingFace/remote URL build points...'
Select-String -Path (Join-Path $src '*.rs'),(Join-Path $src 'asset_pack\*.rs') -Pattern 'huggingface|hf.co|resolve|download_url|pack_url|url|repo|revision|filename|Geometry Dash|title' -CaseSensitive:$false
