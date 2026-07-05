# OxoSteamHook - Steam Ownership Hook System

Minimal Steam hook system inspired by LumaCore to make lua-defined games appear as owned in Steam library.

## Architecture

### Components

1. **dwmapi.dll** - Proxy DLL that forwards Windows DWM API calls and loads OxoSteamCore.dll
2. **xinput1_4.dll** - Backup proxy DLL (optional)
3. **OxoSteamCore.dll** - Main hook library with Microsoft Detours

### How It Works

1. Proxy DLL (dwmapi.dll) is placed in Steam root directory
2. Steam loads dwmapi.dll on startup (before any game code)
3. Proxy forwards all DWM API calls to system DLL
4. Proxy loads OxoSteamCore.dll which installs hooks
5. OxoSteamCore watches `config/stplug-in/*.lua` files
6. When lua file added: parses AppID and depot keys
7. Hooks inject AppIDs into Package 0 (Steam's base subscription)
8. CheckAppOwnership hook fakes ownership for lua-tracked apps
9. GetSubscribedApps hook adds apps to library list
10. Games appear in Steam library as owned

### Lua File Format

```lua
-- HelloKitty.lua (place in Steam/config/stplug-in/)
addappid(2495100)  -- Main game AppID
adddepot(2495101, "HEXKEY")  -- Depot ID + decryption key
adddlc(2495100, 2495200)  -- DLC AppID
```

## Building

### Requirements

- CMake 3.20+
- Visual Studio 2022 (MSVC)
- 64-bit target only

### Build Commands

```powershell
cd E:\007Launcher\native\steam_hook
mkdir build
cd build
cmake .. -G "Visual Studio 17 2022" -A x64
cmake --build . --config Release
```

Output DLLs will be in `build/bin/Release/`:
- `dwmapi.dll`
- `xinput1_4.dll`
- `OxoSteamCore.dll`

## Installation

**WARNING: Modifying Steam is against ToS. Use at your own risk.**

1. Close Steam completely
2. Copy DLLs to Steam root directory (e.g., `C:\Program Files (x86)\Steam\`)
3. Create `config\stplug-in\` directory if not exists
4. Place lua files in `stplug-in\` directory
5. Restart Steam
6. Games should appear in library

## Status

### ✅ Completed (Task #2)
- [x] Project structure
- [x] CMakeLists with Detours + Lua
- [x] Proxy DLLs (dwmapi, xinput)
- [x] Lua file watcher and parser
- [x] Hook stubs (LoadPackage, CheckAppOwnership, GetSubscribedApps)

### 🚧 TODO (Tasks #3-8)
- [ ] Pattern scanning to find Steam functions
- [ ] Actual Detours hooking (currently placeholder)
- [ ] Rust installer in src-tauri
- [ ] Frontend UI
- [ ] Test with Hello Kitty Island Adventure

## Pattern Scanning

LumaCore uses network-fetched TOML files with per-build byte patterns. For simplicity, we need to implement:

1. Read Steam build ID from `steam.exe`
2. Scan steamclient64.dll for function signatures
3. Install Detours hooks at found addresses

Example patterns (from LumaCore):
- `LoadPackage`: `48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18`
- `CheckAppOwnership`: `40 53 48 83 EC 30 48 8B D9 E8 ?? ?? ?? ??`

## Credits

- Inspired by [LumaCore](https://github.com/KoriaPolis/LumaCore) and SteaMidra
- Uses [Microsoft Detours](https://github.com/microsoft/Detours)
- Lua 5.4 for config parsing
