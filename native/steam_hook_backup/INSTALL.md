# OxoSteamHook Installation Guide

## ⚠️ WARNING

**Modifying Steam files violates Steam's Terms of Service and may result in account restrictions. Use at your own risk.**

This tool is for educational purposes and personal use only.

## Prerequisites

- Windows 10/11 64-bit
- Steam installed
- Visual Studio 2022 with C++ support
- CMake 3.20+

## Building

1. Open PowerShell in `E:\007Launcher\native\steam_hook\`
2. Run build script:
   ```powershell
   .\build.bat
   ```
3. Wait for compilation (first build downloads dependencies)
4. DLLs will be in `build\bin\Release\`:
   - `dwmapi.dll`
   - `xinput1_4.dll`
   - `OxoSteamCore.dll`

## Installation Steps

### 1. Close Steam Completely

```powershell
# Kill all Steam processes
taskkill /f /im steam.exe
taskkill /f /im steamwebhelper.exe
timeout /t 2
```

### 2. Backup Original DLLs (Optional)

```powershell
cd "C:\Program Files (x86)\Steam"

# Check if dwmapi.dll already exists (shouldn't normally)
if (Test-Path "dwmapi.dll") {
    Move-Item "dwmapi.dll" "dwmapi.dll.bak"
}
```

### 3. Copy Hook DLLs

```powershell
# From build output
copy "E:\007Launcher\native\steam_hook\build\bin\Release\dwmapi.dll" "C:\Program Files (x86)\Steam\"
copy "E:\007Launcher\native\steam_hook\build\bin\Release\xinput1_4.dll" "C:\Program Files (x86)\Steam\"
copy "E:\007Launcher\native\steam_hook\build\bin\Release\OxoSteamCore.dll" "C:\Program Files (x86)\Steam\"
```

### 4. Create Lua Directory

```powershell
cd "C:\Program Files (x86)\Steam"
New-Item -ItemType Directory -Force -Path "config\stplug-in"
```

### 5. Add Game Lua Files

Copy your game lua files to `C:\Program Files (x86)\Steam\config\stplug-in\`

Example for Hello Kitty Island Adventure (AppID 2495100):

```powershell
# Create lua file
@"
addappid(2495100)
-- adddepot(2495101, "YOUR_DEPOT_KEY_HERE")
"@ | Out-File -Encoding utf8 "C:\Program Files (x86)\Steam\config\stplug-in\2495100.lua"
```

### 6. Start Steam and Check

```powershell
# Start Steam
Start-Process "C:\Program Files (x86)\Steam\steam.exe"

# Monitor debug output (requires DebugView from Sysinternals)
# Download: https://docs.microsoft.com/en-us/sysinternals/downloads/debugview
```

## Verification

### Check DebugView Output

Download [DebugView](https://docs.microsoft.com/en-us/sysinternals/downloads/debugview), run as Administrator, and look for:

```
[OxoHook] Initializing OxoSteamCore...
[OxoHook] Steam path: C:\Program Files (x86)\Steam
[OxoHook] Starting pattern scanning...
[OxoHook] Found LoadPackage at 0x...
[OxoHook] Found CheckAppOwnership at 0x...
[OxoHook] Found GetSubscribedApps at 0x...
[OxoHook] Successfully installed 3 hooks!
[OxoHook] Lua file detected: 2495100.lua
```

### Check Steam Library

1. Open Steam
2. Go to Library
3. Game should appear in your library
4. You can install/manage it normally

## Troubleshooting

### Pattern Not Found

```
[OxoHook] WARNING: XXX pattern not found!
```

**Solution:** Patterns need updating for current Steam build.

1. Get Steam build number:
   - Right-click steam.exe → Properties → Details → Product version
2. Update patterns in `core/patterns.h`
3. Rebuild
4. Reinstall

### Game Not Appearing

**Check:**
- ✅ Lua file exists in `config\stplug-in\`
- ✅ Lua file is valid (no syntax errors)
- ✅ DebugView shows hooks installed
- ✅ DebugView shows lua file parsed
- ✅ Steam restarted after DLL installation

### Steam Won't Start

**Rollback:**
```powershell
cd "C:\Program Files (x86)\Steam"
del dwmapi.dll
del xinput1_4.dll
del OxoSteamCore.dll
```

Then start Steam normally.

## Uninstallation

```powershell
# 1. Close Steam
taskkill /f /im steam.exe

# 2. Remove DLLs
cd "C:\Program Files (x86)\Steam"
del dwmapi.dll
del xinput1_4.dll
del OxoSteamCore.dll

# 3. (Optional) Remove lua files
Remove-Item -Recurse "config\stplug-in"

# 4. Start Steam
Start-Process steam.exe
```

## Automated Installation (Coming Soon)

The 0xoLemon Launcher will automate this process:
- Build DLLs
- Install to Steam
- Manage lua files
- Restart Steam
- Verify installation

This is Task #6 of the project.

## Notes

- DLLs must match your system architecture (64-bit)
- Patterns may need updating after Steam updates
- Antivirus may flag DLLs as suspicious (they hook into Steam)
- Keep backups of working DLL builds
