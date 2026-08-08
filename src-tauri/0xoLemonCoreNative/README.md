# 0xoLemon Core-only build

This package contains only the native C/C++ core from the supplied SFF tree. It does **not** build or ship the Python GUI, Web UI, tray UI, store tabs, or any standalone executable.

Double-click `build.bat` on Windows. A successful Release build creates `dist\` containing exactly these four files:

- `0xoCore.dll`
- `0xoPayload.dll`
- `dwmapi.dll`
- `xinput1_4.dll`

Build intermediates and dependency sources stay in `build\` and `.deps\`; `dist\` is wiped before packaging and is verified to contain exactly four DLLs.

The former Python `sff/core` support layer is now represented by native C++ services under `source/sffcore/` and linked directly into `0xoCore.dll` through a CMake OBJECT library. There is no embedded CPython interpreter and the final audit rejects Python runtime imports/artifacts. See `docs/NATIVE_SFF_CORE_PORT.md` for the exact module mapping.

## About `hooks/ui`

`source/hooks/ui/SteamUI.*` is not a standalone user interface. It is an internal hook layer for Steam's own `steamui.dll`, and existing core code calls it directly. It remains linked into `0xoCore.dll`. The SFF frontend/UI source is not included in this package.

## Requirements

- Windows x64
- Visual Studio 2022 with Desktop development with C++
- CMake 3.20+
- Git
- Internet access on first configure

## Build

```bat
build.bat
```

Clean rebuild:

```bat
build.bat --clean
```

The batch file intentionally uses the Visual Studio 2022 x64 generator rather than auto-selecting Ninja, so it can be launched from an ordinary shell without pre-loading `cl.exe`.

Lua 5.5.0 uses two download URLs plus the official SHA-256. This specifically avoids the previous configure failure caused by `www.lua.org:443` timing out.

The native tree contains no remaining legacy-brand text. Public runtime file names are fixed to the four names above.
