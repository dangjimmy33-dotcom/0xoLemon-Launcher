# Test OxoSteamHook status

$steamPath = "C:\Program Files (x86)\Steam"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "OxoSteamHook Status Check" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Check DLLs
Write-Host "📦 DLL Installation:" -ForegroundColor Yellow
$dlls = @("dwmapi.dll", "xinput1_4.dll", "OxoSteamCore.dll")
foreach ($dll in $dlls) {
    $path = Join-Path $steamPath $dll
    if (Test-Path $path) {
        $size = (Get-Item $path).Length
        Write-Host "  ✅ $dll ($([math]::Round($size/1KB,1))KB)" -ForegroundColor Green
    } else {
        Write-Host "  ❌ $dll MISSING" -ForegroundColor Red
    }
}

Write-Host ""

# Check lua file
Write-Host "📝 Lua Files:" -ForegroundColor Yellow
$luaDir = Join-Path $steamPath "config\stplug-in"
if (Test-Path $luaDir) {
    $luaFiles = Get-ChildItem $luaDir -Filter "*.lua"
    if ($luaFiles.Count -gt 0) {
        foreach ($file in $luaFiles) {
            Write-Host "  ✅ $($file.Name)" -ForegroundColor Green
            $content = Get-Content $file.FullName -Raw
            if ($content -match "addappid\((\d+)\)") {
                Write-Host "     AppID: $($matches[1])" -ForegroundColor DarkGray
            }
        }
    } else {
        Write-Host "  ⚠️  No lua files found" -ForegroundColor Yellow
    }
} else {
    Write-Host "  ❌ stplug-in directory not found" -ForegroundColor Red
}

Write-Host ""

# Check Steam process
Write-Host "🎮 Steam Status:" -ForegroundColor Yellow
$steamProc = Get-Process steam -ErrorAction SilentlyContinue
if ($steamProc) {
    Write-Host "  ✅ Steam is running (PID: $($steamProc.Id))" -ForegroundColor Green
    
    # Check if DLLs are loaded
    $steamModules = $steamProc.Modules | Select-Object -ExpandProperty ModuleName
    if ($steamModules -contains "OxoSteamCore.dll") {
        Write-Host "  ✅ OxoSteamCore.dll is LOADED" -ForegroundColor Green
    } else {
        Write-Host "  ❌ OxoSteamCore.dll NOT loaded" -ForegroundColor Red
    }
} else {
    Write-Host "  ⚠️  Steam is not running" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Recommendations:" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

if ($steamProc -and -not ($steamModules -contains "OxoSteamCore.dll")) {
    Write-Host "❗ DLL not loaded - Restart Steam:" -ForegroundColor Yellow
    Write-Host "   taskkill /f /im steam.exe" -ForegroundColor DarkGray
    Write-Host "   Start-Process 'C:\Program Files (x86)\Steam\steam.exe'" -ForegroundColor DarkGray
}

Write-Host ""
Write-Host "📊 Check DebugView for these messages:" -ForegroundColor Yellow
Write-Host "   [OxoHook] Initializing..." -ForegroundColor DarkGray
Write-Host "   [OxoHook] Starting pattern scanning..." -ForegroundColor DarkGray
Write-Host "   [OxoHook] Found LoadPackage at 0x..." -ForegroundColor DarkGray
Write-Host "   [OxoHook] Successfully installed N hooks!" -ForegroundColor DarkGray
