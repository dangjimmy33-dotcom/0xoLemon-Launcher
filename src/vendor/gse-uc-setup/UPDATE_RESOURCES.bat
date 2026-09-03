@echo off
setlocal EnableExtensions
cd /d "%~dp0"
title GSE / UC Setup - Update Runtime Resources

echo ============================================================
echo   GSE / UC Setup V1.8.3 - Resource Updater
 echo   This is the ONLY build helper that accesses GitHub.
echo ============================================================
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%CD%\FETCH_V18_RESOURCES.ps1" -Strict
if errorlevel 1 (
    echo.
    echo [ERROR] Resource refresh failed. Existing resources were left in place where possible.
    pause
    exit /b 1
)
echo.
echo [OK] Runtime resources refreshed.
pause
exit /b 0
