# Script to minify Firebase credentials JSON for Render deployment

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Firebase Credentials Minifier" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Prompt for file path
$filePath = Read-Host "Enter path to Firebase credentials JSON file"

if (-not (Test-Path $filePath)) {
    Write-Host "ERROR: File not found: $filePath" -ForegroundColor Red
    exit 1
}

Write-Host "Reading JSON file..." -ForegroundColor Yellow
$json = Get-Content $filePath -Raw

Write-Host "Minifying..." -ForegroundColor Yellow
$minified = $json -replace '\s+', ' '

Write-Host "Copying to clipboard..." -ForegroundColor Yellow
$minified | Set-Clipboard

Write-Host ""
Write-Host "✅ SUCCESS! Minified JSON copied to clipboard" -ForegroundColor Green
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Cyan
Write-Host "1. Go to Render: https://dashboard.render.com/web/srv-ctmcj2u8ii6s73buikag/env" -ForegroundColor White
Write-Host "2. Add/Update environment variable" -ForegroundColor White
Write-Host "3. Paste from clipboard (Ctrl+V)" -ForegroundColor White
Write-Host "4. Click 'Save Changes'" -ForegroundColor White
Write-Host ""

# Show snippet
Write-Host "JSON snippet (first 100 chars):" -ForegroundColor Cyan
Write-Host ($minified.Substring(0, [Math]::Min(100, $minified.Length))) -ForegroundColor Gray
Write-Host "..." -ForegroundColor Gray
Write-Host ""
