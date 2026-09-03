## What is this?
This is a build of the `experimental` version of the emu in `steamclient` mode, with an included loader which was originally [written by Rat431](https://github.com/Rat431/ColdAPI_Steam/tree/master/src/ColdClientLoader) and later modified to suit the needs of this emu.  

The backend `.dll/.so` of Steam is a library called `steamclient`, this build will act as a `steamclient` allowing you to retain the original `steam_api(64).dll`. See both the regular and experimental readmes for how to configure it.

---

**Note** that all emu config files should be put beside the `steamclient(64).dll`  

You do not need to create a `steam_interfaces.txt` file for the `steamclient` version of the emu

---

## How to use it?
1. Copy the following files to any folder:  
   * `steamclient.dll`
   * `steamclient64.dll`
   * `ColdClientLoader.ini`
   * `steamclient_loader.exe`

2. While it is not mandatory, it is highly recommended to copy the relevant `GameOverlayRenderer` dll  
   This is recommended because some apps check for the existence of this dll, either on disk, on inside their memory space, otherwise they'll trigger custom protection  
   When in doubt, just copy both dlls:  
   * `GameOverlayRenderer.dll`: for 32-bit apps
   * `GameOverlayRenderer64.dll`: for 64-bit apps

3. Edit `ColdClientLoader.ini` — at minimum set `AppId` and `Exe`, then configure any optional settings:  

   | Setting | Section | Description |
   |---------|---------|-------------|
   | `AppId` | `[default]` | **Required.** Steam App ID for the game |
   | `Exe` | `[default]` | **Required.** Path to the game executable (full or relative to the loader) |
   | `ExeRunDir` | `[default]` | Working directory; defaults to the folder containing `Exe` |
   | `ExeCommandLine` | `[default]` | Extra arguments for the game, e.g. `-dx11 -windowed` |
   | `SteamClientDll` | `[default]` | Path to `steamclient.dll` (full or relative to loader) |
   | `SteamClient64Dll` | `[default]` | Path to `steamclient64.dll` (full or relative to loader) |
   | `ForceInjectSteamClient` | `[default]` | Force-inject `steamclient(64).dll` instead of letting the game load it automatically |
   | `ForceInjectGameOverlayRenderer` | `[default]` | Force-inject `GameOverlayRenderer(64).dll` (expected beside `steamclient(64).dll`) |
   | `ResumeByDebugger` | `[default]` | `1`/`y`/`true` — pause main thread on spawn for debugger attachment; resume and set the entry breakpoint manually from the debugger |
   | `DllsToInjectFolder` | `[default]` | Folder of DLLs to inject at startup; architecture is auto-detected (mismatches skipped); path is full or relative to loader |
   | `IgnoreInjectionError` | `[default]` | `1`/`y`/`true` — suppress error dialogs on injection failure |
   | `IgnoreLoaderArchDifference` | `[default]` | Suppress the arch-mismatch warning (injection silently fails when loader and app architectures differ) |
   | `Mode` | `[Persistence]` | `0` = disabled · `1` = loader stays running until you press OK · `2` = setup-only, launch game manually then press OK (rename loader to `steam.exe`, run as admin) |



**Note** that any arguments passed to `steamclient_loader.exe` via command line will be passed to the target `.exe`.  
Example: `steamclient_loader.exe` `-dx11`  
If the additional exe arguments were both: passed via command line and set in the `.ini` file, then both will be concatenated and passed to the exe.  
This allows the loader to be used/called from other external apps which set additional args.  

## Using `DllsToInjectFolder`
The folder specified by this identifier should contain the dll files you'd like to inject in the app earlier during its creation.  
All the subfolders inside this folder will be traversed recursively, and the dll files inside these subfolders will be loaded/injected.  

Additionally, you can create a file called `load_order.txt` inside your folder (root level, not inside any subdir), mentioning on each line the dll files to inject.  
The order of the lines will instruct the loader which dll to inject first, the dll mentioned on the first line will be injected first and so on.  
Each line inside this file has to be the relative path of your target dll, and it should be relative to your folder. Check the example.  

If this file is created then the loader will only inject the dll files mentioned inside it, otherwise it will attempt to find all valid dll files and inject them.  

---

## Using `extra_dlls` 
This folder contains an experimental dll which, when injected, will attempt to patch the Stub DRM in memory, mainly for newer variants but it also works on some of the older ones.  

This isn't a complete solution, just a different method.  
This dll is meant to be injected during **start-up** only, it must **NOT** be placed inside `.\steam_settings\load_dlls`, otherwise it would cause a huge FPS drop.  

---

## Using `GameOverlayRenderer` 
Some apps verify the existence of this dll, either on disk, or inside their memory space, that's why this dll exists.  
It is **NOT** recommended to ignore this dll.  

## Recommended path for apps/games
Some apps check if their **root folder** is inside `steamapps/common`, so it is recommended to create this directory `steamapps/common` and put the app's **folder** there.  
Example: `~/my apps/steamapps/common/my app/`  
Example: `D:\my apps\steamapps\common\my app\`  

## Mods paths (source-engine games on Windows)
On Windows, the registry key `SourceModInstallPath` is changed to the folder containing the loader.  
```
Registry path:  HKEY_CURRENT_USER\SOFTWARE\Valve\Steam
Registry key:   SourceModInstallPath
Original value: C:\Program Files (x86)\Steam\steamapps\sourcemods
New value:      <FOLDER CONTAINING THE LOADER>
```

This affects source-engine mods  

## Linux
On Linux, you may need to set the environment variable `PROTON_DISABLE_LSTEAMCLIENT=1` when running `steamclient_loader.exe`, depending on the Proton variant you are using (e.g. Proton GE run through Lutris or UMU needs it).
