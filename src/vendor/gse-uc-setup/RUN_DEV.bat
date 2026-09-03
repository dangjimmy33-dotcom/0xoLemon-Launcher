@echo off
setlocal EnableExtensions
cd /d "%~dp0"

echo Preparing native runtime resources...
powershell -NoProfile -ExecutionPolicy Bypass -File "%CD%\FETCH_RUNTIME_RESOURCES.ps1" || exit /b 1

if not exist ".venv\Scripts\python.exe" (
    py -3.13 -m venv .venv 2>nul || py -3.12 -m venv .venv 2>nul || py -3.11 -m venv .venv 2>nul || py -3.10 -m venv .venv || exit /b 1
    call .venv\Scripts\python.exe -m pip install -r requirements.txt || exit /b 1
)
call .venv\Scripts\python.exe app.py
