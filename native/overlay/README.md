# Overlay Native Module

This directory contains the C++ source code for the in-game overlay DLL.
It must be compiled separately for both x86 and x64 architectures.

## Build Requirements
- CMake 3.20+
- Visual Studio 2022 (MSVC v143 toolset)
- Windows SDK 10.0+

## Build Instructions
```powershell
# x64
cmake -B build64 -A x64
cmake --build build64 --config Release

# x86
cmake -B build32 -A Win32
cmake --build build32 --config Release
```

## Output
- `build64/Release/overlay64.dll`
- `build32/Release/overlay32.dll`
