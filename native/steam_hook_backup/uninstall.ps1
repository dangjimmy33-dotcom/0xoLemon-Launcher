# Uninstall OxoSteamHook from Steam

$steamPath = "C:\Program Files (x86)\Steam"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Uninstalling OxoSteamHook..." -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Close Steam
Write-Host "Closing Steam..." -ForegroundColor Yellow
taskkill /f /im steam.exe 2>$null
taskkill /f /im steamwebhelper.exe 2>$null
Start-Sleep -Seconds 2

# Remove DLLs
Write-Host "Removing hook DLLs..." -ForegroundColor Yellow
$files = @("dwmapi.dll", "xinput1_4.dll", "OxoSteamCore.dll")
foreach ($file in $files) {
    $path = Join-Path $steamPath $file
    if (Test-Path $path) {
        Remove-Item $path -Force
        Write-Host "  ✅ Removed $file" -ForegroundColor Green
    } else {
        Write-Host "  ⚠️  $file not found" -ForegroundColor DarkYellow
    }
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Uninstall complete!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Steam is now clean. You can start it normally."
Write-Host ""
Write-Host "To restore backup (if needed):" -ForegroundColor Yellow
Get-ChildItem "$steamPath\_oxo_backup_*" -Directory | ForEach-Object {
    Write-Host "  Copy-Item '$($_.FullName)\*' '$steamPath\' -Force" -ForegroundColor DarkGray
}
