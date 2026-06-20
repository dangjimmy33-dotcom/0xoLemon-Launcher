param (
    [string]$CommitMessage = "Auto build and release"
)

Write-Host "=== STEP 1: REMOVE INVALID GITHUB_TOKEN (IF APPLICABLE) ===" -ForegroundColor Cyan
if (Test-Path Env:\GITHUB_TOKEN) {
    Remove-Item Env:\GITHUB_TOKEN
    Write-Host "Removed GITHUB_TOKEN from current session." -ForegroundColor Yellow
}

Write-Host "`n=== STEP 2: BUILD TAURI APP ===" -ForegroundColor Cyan
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw "C:\Users\conte\.tauri\0xolemon.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "Thanh@12345"

Write-Host "Running npm run tauri build... (This might take a few minutes)" -ForegroundColor Yellow
npm run tauri build
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: Tauri build failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "Build successful!" -ForegroundColor Green

Write-Host "`n=== STEP 3: PUSH TO GITHUB ===" -ForegroundColor Cyan
git add .
git commit -m $CommitMessage
git push
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: GitHub push failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "Push successful!" -ForegroundColor Green

Write-Host "`n=== COMPLETE ===" -ForegroundColor Green
Write-Host "The new build will be automatically processed by GitHub Actions for OTA updates!" -ForegroundColor Yellow
pause
