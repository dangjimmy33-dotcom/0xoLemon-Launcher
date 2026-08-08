@echo off
setlocal EnableExtensions EnableDelayedExpansion

set "ROOT=%~dp0"
set "SOURCE_DIR=%ROOT%source"
set "BUILD_DIR=%ROOT%build"
set "OUT_DIR=%ROOT%dist"
set "LOG_FILE=%ROOT%build_log.txt"
set "DO_CLEAN=0"
set "NO_PAUSE=0"

:parse_args
if "%~1"=="" goto args_done
if /I "%~1"=="--clean" ( set "DO_CLEAN=1" & shift & goto parse_args )
if /I "%~1"=="--no-pause" ( set "NO_PAUSE=1" & shift & goto parse_args )
echo [WARN] Unknown argument: %~1
shift
goto parse_args
:args_done

> "%LOG_FILE%" echo 0xoLemon core-only build started %DATE% %TIME%

echo.
echo ============================================================
echo   0xoLemon CORE-ONLY BUILD - x64 Release
echo   Output: %OUT_DIR%
echo   0xoCore.dll  0xoPayload.dll  dwmapi.dll  xinput1_4.dll
echo ============================================================
echo.

if "%DO_CLEAN%"=="1" (
    echo [STEP] Cleaning build and dependency cache...
    if exist "%BUILD_DIR%\NUL" rmdir /S /Q "%BUILD_DIR%" >> "%LOG_FILE%" 2>&1
    if exist "%ROOT%.deps\NUL" rmdir /S /Q "%ROOT%.deps" >> "%LOG_FILE%" 2>&1
)

if exist "%OUT_DIR%\NUL" rmdir /S /Q "%OUT_DIR%" >> "%LOG_FILE%" 2>&1
mkdir "%OUT_DIR%" >nul 2>&1

set "CMAKE_EXE="
for /f "delims=" %%I in ('where cmake 2^>nul') do if not defined CMAKE_EXE set "CMAKE_EXE=%%I"
if not defined CMAKE_EXE if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Community\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe" set "CMAKE_EXE=%ProgramFiles%\Microsoft Visual Studio\2022\Community\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
if not defined CMAKE_EXE if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Professional\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe" set "CMAKE_EXE=%ProgramFiles%\Microsoft Visual Studio\2022\Professional\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
if not defined CMAKE_EXE if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe" set "CMAKE_EXE=%ProgramFiles%\Microsoft Visual Studio\2022\Enterprise\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
if not defined CMAKE_EXE if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe" set "CMAKE_EXE=%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
if not defined CMAKE_EXE (
    echo [ERROR] CMake not found.
    goto fail
)

where git >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Git not found in PATH. It is required for FetchContent dependencies.
    goto fail
)

echo [1/3] Configure Visual Studio 2022 x64...
"%CMAKE_EXE%" -S "%SOURCE_DIR%" -B "%BUILD_DIR%" -G "Visual Studio 17 2022" -A x64 -DCMAKE_INSTALL_PREFIX="%OUT_DIR%" >> "%LOG_FILE%" 2>&1
if errorlevel 1 goto cmake_fail

echo [2/3] Build Release...
"%CMAKE_EXE%" --build "%BUILD_DIR%" --config Release --parallel >> "%LOG_FILE%" 2>&1
if errorlevel 1 goto cmake_fail

echo [3/3] Package exact four DLLs...
"%CMAKE_EXE%" --install "%BUILD_DIR%" --config Release --prefix "%OUT_DIR%" --component CoreRuntime >> "%LOG_FILE%" 2>&1
if errorlevel 1 goto cmake_fail

set "MISSING=0"
for %%F in ("0xoCore.dll" "0xoPayload.dll" "dwmapi.dll" "xinput1_4.dll") do (
    if not exist "%OUT_DIR%\%%~F" (
        echo [ERROR] Missing %%~F
        set "MISSING=1"
    )
)
if "!MISSING!"=="1" goto fail

set /a DLL_COUNT=0
for %%F in ("%OUT_DIR%\*.dll") do if exist "%%~fF" set /a DLL_COUNT+=1
if not "!DLL_COUNT!"=="4" (
    echo [ERROR] dist has !DLL_COUNT! DLL files; expected exactly 4.
    dir /b "%OUT_DIR%"
    goto fail
)

set /a FILE_COUNT=0
for /f "delims=" %%F in ('dir /b /a-d "%OUT_DIR%" 2^>nul') do set /a FILE_COUNT+=1
if not "!FILE_COUNT!"=="4" (
    echo [ERROR] dist has !FILE_COUNT! files; expected exactly 4.
    dir /b "%OUT_DIR%"
    goto fail
)

rem Reject accidental Python/runtime dependencies in the final native core.
where dumpbin >nul 2>&1
if not errorlevel 1 (
    dumpbin /dependents "%OUT_DIR%\0xoCore.dll" 2^>nul | findstr /I "python3" >nul
    if not errorlevel 1 (
        echo [ERROR] 0xoCore.dll imports a Python runtime; core-only package must be native.
        goto fail
    )
)
for /r "%OUT_DIR%" %%F in (*.py *.pyc *.pyd) do (
    echo [ERROR] Python artifact found in dist: %%~fF
    goto fail
)

echo.
echo [OK] Exact 4-DLL core package created:
dir /b "%OUT_DIR%"
echo.
echo %OUT_DIR%
>> "%LOG_FILE%" echo SUCCESS
if "%NO_PAUSE%"=="0" pause
endlocal
exit /b 0

:cmake_fail
echo [ERROR] CMake/MSBuild failed. See build_log.txt
:fail
>> "%LOG_FILE%" echo FAILED at %DATE% %TIME%
if "%NO_PAUSE%"=="0" pause
endlocal
exit /b 1
