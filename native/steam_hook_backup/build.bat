@echo off
REM Build script for OxoSteamHook

echo ============================================
echo Building OxoSteamHook
echo ============================================

if not exist build mkdir build
cd build

echo.
echo [1/3] Configuring CMake...
cmake .. -G "Visual Studio 17 2022" -A x64
if %errorlevel% neq 0 (
    echo ERROR: CMake configuration failed
    pause
    exit /b 1
)

echo.
echo [2/3] Building Release...
cmake --build . --config Release
if %errorlevel% neq 0 (
    echo ERROR: Build failed
    pause
    exit /b 1
)

echo.
echo [3/3] Build complete!
echo.
echo Output DLLs:
echo   - build\bin\Release\dwmapi.dll
echo   - build\bin\Release\xinput1_4.dll
echo   - build\bin\Release\OxoSteamCore.dll
echo.
echo ============================================
echo Next Steps:
echo 1. Close Steam completely
echo 2. Copy DLLs to Steam root directory
echo 3. Create config\stplug-in\ if not exists
echo 4. Add .lua files to stplug-in\
echo 5. Restart Steam
echo ============================================
pause
