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
Write-Host "Push branch successful!" -ForegroundColor Green

Write-Host "`n=== STEP 4: CREATE RELEASE TAG ===" -ForegroundColor Cyan
# Bump version in package.json and create a git tag
$newVersion = npm version patch
Write-Host "Bumped version to $newVersion" -ForegroundColor Yellow

# Need to update tauri.conf.json manually since npm version doesn't touch it automatically
$tauriConf = Get-Content "src-tauri\tauri.conf.json" | ConvertFrom-Json
$tauriConf.version = $newVersion.Replace("v", "")
$tauriConf | ConvertTo-Json -Depth 10 | Set-Content "src-tauri\tauri.conf.json"

git add src-tauri\tauri.conf.json
git commit --amend --no-edit

# Force update the tag to the new commit
git tag -f $newVersion
git push origin $newVersion
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: Push tag failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "Push tag successful!" -ForegroundColor Green

Write-Host "`n=== COMPLETE ===" -ForegroundColor Green
Write-Host "The new build ($newVersion) will be automatically processed by GitHub Actions for OTA updates!" -ForegroundColor Yellow
pause
