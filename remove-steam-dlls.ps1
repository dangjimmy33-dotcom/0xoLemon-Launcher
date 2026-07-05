# Remove 0xo DLLs from Steam directory
# Run this script as Administrator

$steamPath = "C:\Program Files (x86)\Steam"
$files = @("0xoCore.dll", "0xoPayload.dll", "dwmapi.dll", ".0xo-lua-game-mode-enabled")

Write-Host "Removing 0xo DLLs from Steam..." -ForegroundColor Yellow
Write-Host "Steam path: $steamPath" -ForegroundColor Cyan
Write-Host ""

foreach ($file in $files) {
    $fullPath = Join-Path $steamPath $file
    
    if (Test-Path $fullPath) {
        try {
            Remove-Item $fullPath -Force -ErrorAction Stop
            Write-Host "[✓] Deleted: $file" -ForegroundColor Green
        } catch {
            Write-Host "[✗] Failed to delete: $file - $($_.Exception.Message)" -ForegroundColor Red
        }
    } else {
        Write-Host "[ ] Not found: $file" -ForegroundColor Gray
    }
}

Write-Host ""
Write-Host "Done! Press any key to exit..." -ForegroundColor Cyan
$null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
