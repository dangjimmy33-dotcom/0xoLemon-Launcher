$ErrorActionPreference = "Stop"

$ostDir = "E:\OpenSteamTool"
$targetDir = "$PSScriptRoot\resources\ost"

Write-Host "Building OpenSteamTool in $ostDir..."
Push-Location $ostDir
try {
    # Call the build.bat directly, focusing only on Release build for now to speed up
    $env:CONFIGS = "Release"
    cmd.exe /c "build.bat"
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "OpenSteamTool build reported exit code $LASTEXITCODE, but dlls might have built successfully. Checking..."
    }
} finally {
    Pop-Location
}

if (-not (Test-Path $targetDir)) {
    New-Item -ItemType Directory -Path $targetDir | Out-Null
}

$dlls = @("dwmapi.dll", "OpenSteamTool.dll", "xinput1_4.dll")

foreach ($dll in $dlls) {
    $src = "$ostDir\build\Release\$dll"
    if (Test-Path $src) {
        Copy-Item -Path $src -Destination $targetDir -Force
        Write-Host "Copied $dll to $targetDir"
    } else {
        throw "Failed to find built DLL: $src"
    }
}

Write-Host "OpenSteamTool integration build complete."
