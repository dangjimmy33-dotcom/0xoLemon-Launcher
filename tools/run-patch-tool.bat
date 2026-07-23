@echo off
chcp 65001 >nul
cd /d "%~dp0"
title 0xo Patch Fix Builder - Local GUI
color 0A

echo ======================================================
echo  0xo Patch Fix Builder - Local GUI Tool
echo ======================================================
echo.
echo Tool se mo trinh duyet tai http://127.0.0.1:8777
echo Dong cua so nay se tat backend local.
echo.

start http://127.0.0.1:8777

where python >nul 2>nul
if %errorlevel%==0 (
  python 0xo_patch_builder_server.py
  goto :end
)

where py >nul 2>nul
if %errorlevel%==0 (
  py -3 0xo_patch_builder_server.py
  goto :end
)

echo [ERROR] Khong tim thay Python trong PATH.
echo Hay cai Python 3 va tick Add Python to PATH.
pause

:end
