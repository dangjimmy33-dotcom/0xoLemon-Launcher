@echo off
call "E:\visual studio 2022\VC\Auxiliary\Build\vcvars64.bat"
cd /d "%~dp0"
echo Compiling GSE DirectInput8 Bridge DLL...
cl /nologo /O2 /MD /LD /W3 main.cpp /Fe:dinput8.dll /link /DEF:dinput8.def user32.lib
if %ERRORLEVEL% equ 0 (
    echo [SUCCESS] dinput8.dll built successfully!
) else (
    echo [ERROR] Build failed!
)