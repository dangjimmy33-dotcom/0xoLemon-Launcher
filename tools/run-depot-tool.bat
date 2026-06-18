@echo off
chcp 65001 >nul
cd /d "%~dp0"
title 0xo Depot Uploader Studio - Local Tool
color 0A

echo ======================================================
echo  0xo Depot Uploader Studio - Local GUI Tool
echo ======================================================
echo.
echo Tool se mo trinh duyet tai http://127.0.0.1:8776
echo Dong cua so nay se tat backend local.
echo.

where python >nul 2>nul
if %errorlevel%==0 (
  python 0xo_depot_uploader_server.py
  goto :end
)

where py >nul 2>nul
if %errorlevel%==0 (
  py -3 0xo_depot_uploader_server.py
  goto :end
)

echo [ERROR] Khong tim thay Python trong PATH.
echo Hay cai Python 3 va tick Add Python to PATH.
pause

:end
