@echo off
setlocal

REM Path to vcpkg (edit if installed elsewhere)
set VCPKG_ROOT=C:\vcpkg

REM Install dependencies
%VCPKG_ROOT%\vcpkg.exe install tinyxml2:x64-windows cryptopp:x64-windows

REM Clean build folder
if exist build rmdir /s /q build

REM Configure project with CMake
cmake -S . -B build -G "Visual Studio 17 2022" -A x64 ^
    -DCMAKE_TOOLCHAIN_FILE=%VCPKG_ROOT%\scripts\buildsystems\vcpkg.cmake ^
    -DVCPKG_TARGET_TRIPLET=x64-windows -DCMAKE_BUILD_TYPE=Release

REM Build the project in Release mode
cmake --build build --config Release

echo.
echo ==============================================
echo Build finished! DLL should be in:
echo   build\Release\TokenDumper.dll
echo ==============================================
pause
