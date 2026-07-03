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

# Sync version to tauri.conf.json using Node.js to preserve JSON formatting
node -e "const fs=require('fs'); const p=require('./package.json'); const t=JSON.parse(fs.readFileSync('./src-tauri/tauri.conf.json','utf8')); t.version=p.version; fs.writeFileSync('./src-tauri/tauri.conf.json', JSON.stringify(t, null, 2) + '\n');"

Write-Host "`n=== STEP 3.5: UPDATE CHANGELOG ===" -ForegroundColor Cyan
$changelogFile = "src/changelog.json"

# Hiển thị UI Form để nhập changelog
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$form = New-Object System.Windows.Forms.Form
$form.Text = "Nhập Changelog cho phiên bản $newVersion"
$form.Size = New-Object System.Drawing.Size(600,400)
$form.StartPosition = 'CenterScreen'
$form.TopMost = $true

$label = New-Object System.Windows.Forms.Label
$label.Location = New-Object System.Drawing.Point(15,15)
$label.Size = New-Object System.Drawing.Size(550,40)
$label.Font = New-Object System.Drawing.Font("Arial", 10)
$label.Text = "Nhập các thay đổi trong phiên bản này (mỗi dòng một thay đổi):`n(Bỏ trống và bấm OK nếu không muốn cập nhật changelog)"

$textBox = New-Object System.Windows.Forms.TextBox
$textBox.Location = New-Object System.Drawing.Point(15,60)
$textBox.Size = New-Object System.Drawing.Size(550,240)
$textBox.Multiline = $true
$textBox.ScrollBars = 'Vertical'
$textBox.Font = New-Object System.Drawing.Font("Consolas", 11)

$okButton = New-Object System.Windows.Forms.Button
$okButton.Location = New-Object System.Drawing.Point(465,315)
$okButton.Size = New-Object System.Drawing.Size(100,30)
$okButton.Text = "Lưu & Tiếp tục"
$okButton.DialogResult = [System.Windows.Forms.DialogResult]::OK

$form.Controls.Add($label)
$form.Controls.Add($textBox)
$form.Controls.Add($okButton)
$form.AcceptButton = $okButton

Write-Host "Waiting for changelog input via UI window..." -ForegroundColor Yellow
$result = $form.ShowDialog()

if ($result -eq [System.Windows.Forms.DialogResult]::OK -and -not [string]::IsNullOrWhiteSpace($textBox.Text)) {
    $changes = @($textBox.Text -split "`n" | Where-Object { $_ -match '\S' } | ForEach-Object { $_.Trim() -replace '^- ', '' -replace '^\* ', '' })
    
    if ($changes.Count -gt 0) {
        Write-Host "Changelog entered ($($changes.Count) items). Updating $changelogFile..." -ForegroundColor Green
        
        # Read existing changelog
        $jsonContent = Get-Content $changelogFile -Raw -Encoding UTF8 | ConvertFrom-Json
        
        # Create new entry
        $newEntry = [ordered]@{
            version = $newVersion.TrimStart('v')
            date = (Get-Date -Format 'yyyy-MM-dd')
            changes = @($changes)
        }
        
        # Prepend new entry
        $updatedJson = @($newEntry) + $jsonContent
        
        # Save back to file
        $updatedJson | ConvertTo-Json -Depth 10 | Set-Content $changelogFile -Encoding UTF8
        
        Write-Host "Changelog updated successfully." -ForegroundColor Green
    } else {
        Write-Host "No changelog entered. Skipping changelog update." -ForegroundColor Yellow
    }
} else {
    Write-Host "No changelog entered. Skipping changelog update." -ForegroundColor Yellow
}

# Commit version bump (package.json + tauri.conf.json + changelog.json together)
# Use ErrorAction Ignore for package-lock.json in case it doesn't exist
git add package.json src-tauri\tauri.conf.json src\changelog.json
if (Test-Path "package-lock.json") { git add package-lock.json }
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
