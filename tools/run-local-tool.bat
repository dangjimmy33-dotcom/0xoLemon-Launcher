@echo off
setlocal
cd /d "%~dp0"
title 0xo Asset Builder Local GUI

echo Starting 0xo Asset Builder Local GUI...
echo.

where py >nul 2>nul
if %errorlevel%==0 (
  py -3 "%~dp00xo_asset_builder_server.py"
  goto :end
)

where python >nul 2>nul
if %errorlevel%==0 (
  python "%~dp00xo_asset_builder_server.py"
  goto :end
)

echo [ERROR] Python was not found. Install Python 3, then run this file again.
echo https://www.python.org/downloads/
pause

:end
endlocal
