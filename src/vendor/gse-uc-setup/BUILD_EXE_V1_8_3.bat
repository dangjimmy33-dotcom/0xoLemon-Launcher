@echo off
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0"
title GSE / UC Setup - Build EXE V1.8.3

set "PYPI=https://pypi.org/simple"
set "PYSELECT="
set "PYDESC="
set "BOOTSTRAPPED="

set "PIP_NO_INDEX="
set "PIP_INDEX_URL="
set "PIP_EXTRA_INDEX_URL="

echo ============================================================
echo   GSE / UC Setup V1.8.3 - Windows EXE Builder
echo ============================================================
echo.
echo Requirements:
echo   - Windows x64
echo   - regular 64-bit CPython 3.10-3.14
echo   - NOT the free-threaded ^(*t / Py_GIL_DISABLED^) build
echo.
echo PySide6 currently ships normal ABI Windows wheels.
echo Free-threaded CPython requires separately built extension wheels.
echo.

where py >nul 2>nul
if errorlevel 1 (
    echo [ERROR] Python Launcher / Python Install Manager ^(py.exe^) was not found.
    echo Install regular 64-bit CPython 3.13 from python.org and rerun.
    goto :fail
)

call :find_regular_python
if defined PYSELECT goto :python_found

echo [0/8] No compatible regular CPython was found.
echo.
echo Detected Python installations:
py -0p 2>nul
echo.
echo The detected Python may be a free-threaded build ^(for example cp314t^).
echo V1.8.3 will try to install regular CPython 3.13 x64 automatically.
echo.

py install 3.13 >nul 2>nul
call :find_regular_python
if defined PYSELECT (
    set "BOOTSTRAPPED=1"
    goto :python_found
)

where winget >nul 2>nul
if errorlevel 1 goto :bootstrap_failed

echo       Python Install Manager unavailable; trying winget...
winget install --id Python.Python.3.13 -e --architecture x64 --scope user --silent --accept-package-agreements --accept-source-agreements
if errorlevel 1 goto :bootstrap_failed

call :find_regular_python
if defined PYSELECT (
    set "BOOTSTRAPPED=1"
    goto :python_found
)

:bootstrap_failed
echo.
echo [ERROR] Could not bootstrap regular 64-bit CPython automatically.
echo Install regular CPython 3.13 x64 and rerun.
echo.
echo Modern Python Install Manager:
echo     py install 3.13
echo.
echo winget:
echo     winget install -e --id Python.Python.3.13 --architecture x64
goto :fail

:python_found
echo [0/8] Compatible Python selected: !PYDESC!
%PYSELECT% -c "import sys,sysconfig,struct,platform; ft=(sysconfig.get_config_var('Py_GIL_DISABLED')==1); print('      Executable    :',sys.executable); print('      Version       :',sys.version.split()[0]); print('      Architecture  :',struct.calcsize('P')*8,'bit'); print('      Platform      :',platform.machine()); print('      ABI flags     :',getattr(sys,'abiflags','')); print('      Free-threaded :','YES' if ft else 'NO'); raise SystemExit(1 if ft else 0)"
if errorlevel 1 (
    echo [ERROR] Internal selector error: free-threaded Python slipped through.
    goto :fail
)
if defined BOOTSTRAPPED echo       Regular CPython 3.13 was installed automatically.

if exist ".venv\Scripts\python.exe" (
    ".venv\Scripts\python.exe" -c "import sys,sysconfig,struct; ft=(sysconfig.get_config_var('Py_GIL_DISABLED')==1); ok=(struct.calcsize('P')*8==64 and (3,10)<=sys.version_info[:2]<(3,15) and not ft); raise SystemExit(0 if ok else 1)" >nul 2>nul
    if errorlevel 1 (
        echo [1/8] Existing virtual environment uses incompatible ABI - recreating...
        rmdir /s /q ".venv"
        if exist ".venv" (
            echo [ERROR] Could not remove .venv. Close programs using it and retry.
            goto :fail
        )
    ) else (
        echo [1/8] Existing regular 64-bit virtual environment: OK
    )
)

if not exist ".venv\Scripts\python.exe" (
    echo [1/8] Creating virtual environment from regular CPython...
    %PYSELECT% -m venv .venv
    if errorlevel 1 goto :fail
)

".venv\Scripts\python.exe" -c "import sys,sysconfig,struct; ft=(sysconfig.get_config_var('Py_GIL_DISABLED')==1); print('      venv Python   :',sys.version.split()[0]); print('      venv ABI flags:',getattr(sys,'abiflags','')); print('      Free-threaded :','YES' if ft else 'NO'); assert struct.calcsize('P')*8==64 and not ft"
if errorlevel 1 goto :fail

echo [2/8] Updating pip from official PyPI...
".venv\Scripts\python.exe" -m pip install --disable-pip-version-check --index-url "%PYPI%" --upgrade pip
if errorlevel 1 goto :pip_help

echo [3/8] Verifying a compatible PySide6 wheel exists...
if exist ".wheelcheck" rmdir /s /q ".wheelcheck"
mkdir ".wheelcheck"
".venv\Scripts\python.exe" -m pip download --disable-pip-version-check --index-url "%PYPI%" --only-binary=:all: --no-deps --dest ".wheelcheck" "PySide6==6.11.2"
if errorlevel 1 goto :pip_help
rmdir /s /q ".wheelcheck"

echo [4/8] Installing project dependencies from official PyPI...
".venv\Scripts\python.exe" -m pip install --disable-pip-version-check --index-url "%PYPI%" --only-binary PySide6,PySide6_Essentials,PySide6_Addons,shiboken6 -r requirements.txt
if errorlevel 1 goto :pip_help

".venv\Scripts\python.exe" -c "import PySide6,PyInstaller; print('      PySide6    :',PySide6.__version__); print('      PyInstaller:',PyInstaller.__version__)"
if errorlevel 1 goto :pip_help

echo [5/8] Validating existing runtime resources (offline, no refresh)...
if not exist "resources\7zip\7za.exe" (
    echo [ERROR] resources\7zip\7za.exe is missing.
    echo Run UPDATE_RESOURCES.bat once, then retry.
    goto :fail
)
if not exist "resources\embedded\gse\regular\x64\steam_api64.dll" (
    echo [ERROR] Full embedded GSE Regular baseline is missing.
    goto :fail
)
if not exist "resources\embedded\gse\steamclient_experimental\GameOverlayRenderer64.dll" (
    echo [ERROR] Full embedded GSE ColdClient baseline is incomplete.
    goto :fail
)
if not exist "resources\embedded\steamless\Steamless.CLI.exe" (
    echo [ERROR] Embedded Steamless CLI is missing.
    goto :fail
)
if not exist "resources\embedded\migrate_gse\migrate_gse.exe" (
    echo [ERROR] Embedded migrate_gse is missing.
    goto :fail
)
dir /s /b "resources\embedded\gse_tools\generate_emu_config.exe" >nul 2>nul
if errorlevel 1 (
    echo [ERROR] Embedded official gse_fork_tools generator is missing.
    echo Run UPDATE_RESOURCES.bat once, then retry.
    goto :fail
)
if not exist "resources\embedded\uc_online" (
    echo [ERROR] Embedded UC Online2 resource folder is missing.
    goto :fail
)
dir /s /b "resources\embedded\uc_online\steam_api64.dll" >nul 2>nul
if errorlevel 1 (
    echo [ERROR] Embedded UC Online2 x64 steam_api64.dll is missing.
    echo Run UPDATE_RESOURCES.bat once, then retry.
    goto :fail
)
if not exist "resources\embedded\rune_steamstub\steamstub_x64.dll" (
    echo [ERROR] Embedded RUNE SteamStub x64 baseline is missing.
    goto :fail
)
echo       Existing resources: OK - no GitHub download performed.

echo [6/8] Running backend tests...
".venv\Scripts\python.exe" -m pytest -q
if errorlevel 1 goto :fail

echo [7/8] Syntax checking project...
".venv\Scripts\python.exe" -m compileall -q app.py gse_autosetup
if errorlevel 1 goto :fail

echo [8/8] Building one-file Windows EXE...
".venv\Scripts\python.exe" -m PyInstaller --noconfirm --clean GSEAutoSetup.spec
if errorlevel 1 goto :fail

echo       Copying portable resources beside EXE...
if exist "dist\resources" rmdir /s /q "dist\resources"
mkdir "dist\resources"
xcopy /E /I /Y /Q "resources\embedded" "dist\resources\embedded" >nul
if errorlevel 1 goto :fail
xcopy /E /I /Y /Q "resources\7zip" "dist\resources\7zip" >nul
if errorlevel 1 goto :fail
if exist "resources\gse_seed" xcopy /E /I /Y /Q "resources\gse_seed" "dist\resources\gse_seed" >nul
if exist "resources\preserve_seed" xcopy /E /I /Y /Q "resources\preserve_seed" "dist\resources\preserve_seed" >nul

echo.
echo ============================================================
echo   BUILD COMPLETE
echo   EXE: %CD%\dist\GSEAutoSetup.exe
echo ============================================================
if exist "%CD%\dist\GSEAutoSetup.exe" explorer "%CD%\dist"
pause
exit /b 0

:find_regular_python
set "PYSELECT="
set "PYDESC="
call :try_python 3.13
if defined PYSELECT exit /b 0
call :try_python 3.12
if defined PYSELECT exit /b 0
call :try_python 3.11
if defined PYSELECT exit /b 0
call :try_python 3.10
if defined PYSELECT exit /b 0
call :try_python 3.14
exit /b 0

:try_python
py -%1 -c "import sys,sysconfig,struct; ft=(sysconfig.get_config_var('Py_GIL_DISABLED')==1); ok=(struct.calcsize('P')*8==64 and (3,10)<=sys.version_info[:2]<(3,15) and not ft and 'windowsapps' not in sys.base_prefix.lower()); raise SystemExit(0 if ok else 1)" >nul 2>nul
if not errorlevel 1 (
    set "PYSELECT=py -%1"
    set "PYDESC=regular CPython %1 x64"
)
exit /b 0

:pip_help
echo.
echo [ERROR] Dependency installation failed.
echo.
echo Python / ABI diagnostics:
".venv\Scripts\python.exe" -c "import sys,sysconfig,struct,platform; print('sys.version     :',sys.version); print('sys.abiflags    :',getattr(sys,'abiflags','')); print('Py_GIL_DISABLED :',sysconfig.get_config_var('Py_GIL_DISABLED')); print('pointer bits    :',struct.calcsize('P')*8); print('platform        :',platform.machine())"
echo.
echo pip configuration:
".venv\Scripts\python.exe" -m pip config debug
echo.
echo pip compatible tags:
".venv\Scripts\python.exe" -m pip debug --verbose
echo.
echo IMPORTANT:
echo   cp314t / abi3t means free-threaded Python.
echo   Current PySide6 Windows wheels are normal ABI wheels, so use
echo   regular CPython instead.
goto :fail

:fail
echo.
echo ============================================================
echo   BUILD FAILED - read the diagnostic above.
echo ============================================================
pause
exit /b 1
