# Build CloudRedirect DLL for 0xoLemon Launcher
# This builds the updated CloudRedirect with error handling, lua sync, achievements, etc.

$ErrorActionPreference = "Stop"

Write-Host "=== Building CloudRedirect (0xoCloudRedirect) ===" -ForegroundColor Cyan
Write-Host ""

# Check CMake
if (!(Get-Command cmake -ErrorAction SilentlyContinue)) {
    Write-Host "ERROR: CMake not found. Please install CMake 3.20+." -ForegroundColor Red
    exit 1
}

# Check Visual Studio Build Tools
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (!(Test-Path $vswhere)) {
    Write-Host "ERROR: Visual Studio 2022 not found. Please install VS2022 Build Tools." -ForegroundColor Red
    exit 1
}

$vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (!$vsPath) {
    Write-Host "ERROR: Visual Studio C++ tools not found." -ForegroundColor Red
    exit 1
}

Write-Host "Found Visual Studio at: $vsPath" -ForegroundColor Green

# Create build directory
$buildDir = "E:\007Launcher\native\cloud_redirect\build"
if (Test-Path $buildDir) {
    Write-Host "Cleaning old build..." -ForegroundColor Yellow
    Remove-Item $buildDir -Recurse -Force
}

New-Item -ItemType Directory -Path $buildDir -Force | Out-Null
Write-Host "Created build directory" -ForegroundColor Green

# Copy Version.props if not exists
$versionProps = "E:\007Launcher\native\cloud_redirect\Version.props"
if (!(Test-Path $versionProps)) {
    Write-Host "Creating Version.props..." -ForegroundColor Yellow
    @"
<?xml version="1.0" encoding="utf-8"?>
<Project>
  <PropertyGroup>
    <ReleaseVersion>2.0.0</ReleaseVersion>
    <ReleasePrerelease>-0xoLemon</ReleasePrerelease>
  </PropertyGroup>
</Project>
"@ | Out-File -FilePath $versionProps -Encoding UTF8
}

# Configure with CMake
Write-Host ""
Write-Host "Configuring with CMake..." -ForegroundColor Cyan
Set-Location $buildDir

cmake .. -G "Visual Studio 17 2022" -A x64
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: CMake configuration failed" -ForegroundColor Red
    exit 1
}

# Build Release
Write-Host ""
Write-Host "Building Release configuration..." -ForegroundColor Cyan
cmake --build . --config Release --parallel
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Build failed" -ForegroundColor Red
    exit 1
}

# Copy DLL to launcher resources
$sourceDll = "$buildDir\Release\cloud_redirect.dll"
$destDir = "E:\007Launcher\src-tauri\resources\cloud_redirect"
$destDll = "$destDir\0xoCloudRedirect.dll"

if (!(Test-Path $sourceDll)) {
    Write-Host "ERROR: cloud_redirect.dll not found at $sourceDll" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "Copying DLL to launcher resources..." -ForegroundColor Cyan
New-Item -ItemType Directory -Path $destDir -Force | Out-Null
Copy-Item $sourceDll $destDll -Force

$dllSize = (Get-Item $destDll).Length
$dllSizeMB = [math]::Round($dllSize / 1MB, 2)

Write-Host ""
Write-Host "=== Build Complete ===" -ForegroundColor Green
Write-Host "DLL Location: $destDll" -ForegroundColor Cyan
Write-Host "DLL Size: $dllSizeMB MB" -ForegroundColor Cyan
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Yellow
Write-Host "1. Update Rust integration code" -ForegroundColor White
Write-Host "2. Add UI in launcher Settings" -ForegroundColor White
Write-Host "3. Test with Steam" -ForegroundColor White
