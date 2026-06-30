param (
    [string]$CommitMessage = "Auto build and release"
)

Write-Host "=== STEP 1: REMOVE INVALID GITHUB_TOKEN (IF APPLICABLE) ===" -ForegroundColor Cyan
if (Test-Path Env:\GITHUB_TOKEN) {
    Remove-Item Env:\GITHUB_TOKEN
    Write-Host "Removed GITHUB_TOKEN from current session." -ForegroundColor Yellow
}

Write-Host "`n=== STEP 2: COMMIT CURRENT CHANGES ===" -ForegroundColor Cyan
git add .
$hasChanges = git status --porcelain
if ($hasChanges) {
    git commit -m $CommitMessage
    Write-Host "Committed changes." -ForegroundColor Green
} else {
    Write-Host "No changes to commit." -ForegroundColor Yellow
}

Write-Host "`n=== STEP 3: BUMP VERSION ===" -ForegroundColor Cyan
# npm version patch: bumps package.json, commits, creates local tag
$newVersion = npm version patch --no-git-tag-version
Write-Host "Bumped version to $newVersion" -ForegroundColor Yellow

# Sync version to tauri.conf.json
$tauriConf = Get-Content "src-tauri\tauri.conf.json" | ConvertFrom-Json
$tauriConf.version = $newVersion.Replace("v", "")
$tauriJson = $tauriConf | ConvertTo-Json -Depth 20
[System.IO.File]::WriteAllText("$PWD\src-tauri\tauri.conf.json", $tauriJson, [System.Text.UTF8Encoding]::new($false))

# Commit version bump (package.json + tauri.conf.json together)
git add package.json package-lock.json src-tauri\tauri.conf.json
git commit -m "chore: bump version to $newVersion"
Write-Host "Version bump committed." -ForegroundColor Green

# Create local tag pointing to this commit
git tag -f $newVersion
Write-Host "Tagged $newVersion locally." -ForegroundColor Green

Write-Host "`n=== STEP 4: BUILD TAURI APP (with new version) ===" -ForegroundColor Cyan
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw "C:\Users\conte\.tauri\0xolemon.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "Thanh@12345"

Write-Host "Running npm run tauri build... (This might take a few minutes)" -ForegroundColor Yellow
npm run tauri build
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: Tauri build failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "Build successful!" -ForegroundColor Green

Write-Host "`n=== STEP 5: PUSH BRANCH + TAG TO GITHUB ===" -ForegroundColor Cyan
# Push the branch (includes all commits up to and including the version bump)
git push
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: GitHub push failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "Push branch successful!" -ForegroundColor Green

# Push the version tag — this triggers GitHub Actions to build & publish the release
git push origin $newVersion
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: Push tag failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "Push tag successful!" -ForegroundColor Green

Write-Host "`n=== COMPLETE ===" -ForegroundColor Green
Write-Host "GitHub Actions will now build and publish the signed release for $newVersion" -ForegroundColor Yellow
Write-Host ""
Write-Host "NOTE: Make sure the following GitHub secrets are set in:" -ForegroundColor Cyan
Write-Host "  https://github.com/dangjimmy33-dotcom/0xoLemon-Launcher/settings/secrets/actions" -ForegroundColor Cyan
Write-Host "  - TAURI_SIGNING_PRIVATE_KEY  (content of C:\Users\conte\.tauri\0xolemon.key)" -ForegroundColor White
Write-Host "  - TAURI_SIGNING_PRIVATE_KEY_PASSWORD  (your key password)" -ForegroundColor White
Write-Host "  - HF_REPOS_JSON  (your HuggingFace repos config)" -ForegroundColor White
pause
