@echo off
setlocal EnableExtensions
cd /d "%~dp0"
title GSE / UC Setup - FAST Build V1.8.3

echo ============================================================
echo   GSE / UC Setup V1.8.3 - FAST/OFFLINE Builder
 echo   No package installation. No GitHub fetch. Uses existing .venv + resources.
echo ============================================================
echo.

if not exist ".venv\Scripts\python.exe" (
    echo [ERROR] .venv is missing. Run BUILD_EXE.bat once to create it.
    goto :fail
)

".venv\Scripts\python.exe" -c "import PySide6,PyInstaller,google.auth,googleapiclient,google_auth_oauthlib; print('PySide6:',PySide6.__version__); print('PyInstaller:',PyInstaller.__version__); print('Google Drive libs: OK')"
if errorlevel 1 (
    echo [ERROR] Build dependencies are missing from .venv. Run BUILD_EXE.bat once.
    goto :fail
)

echo [1/5] Validating existing resources...
if not exist "resources\7zip\7za.exe" goto :missing_resources
if not exist "resources\embedded\gse\regular\x64\steam_api64.dll" goto :missing_resources
if not exist "resources\embedded\gse\steamclient_experimental\GameOverlayRenderer64.dll" goto :missing_resources
if not exist "resources\embedded\steamless\Steamless.CLI.exe" goto :missing_resources
if not exist "resources\embedded\migrate_gse\migrate_gse.exe" goto :missing_resources
if not exist "resources\embedded\rune_steamstub\steamstub_x64.dll" goto :missing_resources
dir /s /b "resources\embedded\gse_tools\generate_emu_config.exe" >nul 2>nul
if errorlevel 1 goto :missing_resources
dir /s /b "resources\embedded\uc_online\steam_api64.dll" >nul 2>nul
if errorlevel 1 goto :missing_resources
echo       Resources: OK. No network access used.

echo [2/5] Running tests...
".venv\Scripts\python.exe" -m pytest -q
if errorlevel 1 goto :fail

echo [3/5] Syntax checking...
".venv\Scripts\python.exe" -m compileall -q app.py gse_autosetup
if errorlevel 1 goto :fail

echo [4/5] Building EXE...
".venv\Scripts\python.exe" -m PyInstaller --noconfirm --clean GSEAutoSetup.spec
if errorlevel 1 goto :fail

echo [5/5] Copying resources beside EXE...
if exist "dist\resources" rmdir /s /q "dist\resources"
mkdir "dist\resources"
xcopy /E /I /Y /Q "resources\embedded" "dist\resources\embedded" >nul
if errorlevel 1 goto :fail
xcopy /E /I /Y /Q "resources\7zip" "dist\resources\7zip" >nul
if errorlevel 1 goto :fail
if exist "resources\gse_seed" xcopy /E /I /Y /Q "resources\gse_seed" "dist\resources\gse_seed" >nul
if exist "resources\preserve_seed" xcopy /E /I /Y /Q "resources\preserve_seed" "dist\resources\preserve_seed" >nul
if exist "resources\google" xcopy /E /I /Y /Q "resources\google" "dist\resources\google" >nul

echo.
echo ============================================================
echo   FAST BUILD COMPLETE
echo   EXE      : %CD%\dist\GSEAutoSetup.exe
echo   RESOURCES: %CD%\dist\resources
echo ============================================================
explorer "%CD%\dist"
pause
exit /b 0

:missing_resources
echo [ERROR] Required runtime resources are incomplete.
echo Run UPDATE_RESOURCES.bat once. After that, FAST builds stay offline.
goto :fail

:fail
echo.
echo ============================================================
echo   BUILD FAILED - read the diagnostic above.
echo ============================================================
pause
exit /b 1
