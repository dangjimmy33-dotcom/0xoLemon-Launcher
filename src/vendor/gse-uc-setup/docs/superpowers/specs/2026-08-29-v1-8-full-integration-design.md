# GSE / UC Setup V1.8 — Full Integration Design

Date: 2026-08-29
Status: Approved in chat; written-spec review pending before implementation
Baseline: GSE Auto Setup V1.7.1

## 1. Goal

V1.8 turns the current GSE-only utility into a two-engine portable setup utility with a shared resource/update layer:

- **GSE engine** for Regular, Experimental, and official ColdClient/steamclient_experimental deployments.
- **UC Online2 engine** for Steam-client-backed Spacewar/custom AppID operation.
- **SteamStub handling** through Steamless, RUNE SteamStub patcher, UC runtime patching, or Disabled.
- **Hybrid resources**: a working baseline is bundled into the packaged EXE, while component updates live beside the EXE under `resources/updates/`.
- **No large cache or backup trees in `%LOCALAPPDATA%`**.
- **Fluent/Mica glass UI** with more explicit controls for engine, connectivity, overlay, saves, DRM handling, migration, and resource updates.

The implementation must preserve the existing V1.7.1 requirements: adjacent original Steam API backup, game-local restore manifest, official GSE config preference, multilingual achievements, and DPAPI-protected API-key persistence.

## 2. Source Inputs

The implementation is based on these user-provided inputs:

- `GSE_Auto_Setup_V1_7_1(3).zip`
- `uc-online2-main.zip`
- `RUNEAutoCracker-main(1).zip`
- `emu-win-release.zip`
- `migrate_gse.zip`

Upstream public projects are used for update discovery and compatibility checks. User-provided binaries remain the initial embedded baseline where no newer verified official binary is prepared during the Windows build.

## 3. Non-goals

V1.8 will not:

- silently mix GSE and UC Online2 in the same deployment;
- silently drop a proxy DLL over an existing third-party proxy;
- silently migrate old Goldberg settings;
- silently overwrite a pre-existing `.bak` that the tool did not create;
- call a GSE `offline=0` preset “Steam Online”;
- keep large archives/extracted packages in `%LOCALAPPDATA%`;
- rebuild third-party C/C++ tools at runtime.

## 4. Top-level Architecture

```text
GSEAutoSetup.exe
│
├─ EmbeddedResourceProvider
│  ├─ gse/
│  ├─ gse_tools/
│  ├─ gse_coldclient/
│  ├─ migrate_gse/
│  ├─ uc_online2/
│  ├─ steamless/
│  ├─ rune_steamstub/
│  └─ 7zip/
│
├─ PortableUpdateProvider
│  └─ resources/updates/<component>/
│
├─ ResourceResolver
│  └─ update -> embedded baseline
│
├─ DeploymentCoordinator
│  ├─ GseDeploymentEngine
│  └─ UcOnlineDeploymentEngine
│
├─ DrmCoordinator
│  ├─ SteamlessHandler
│  ├─ RuneSteamStubHandler
│  └─ UcRuntimeSteamStubHandler
│
├─ BackupRestoreManager
├─ GseMigrationService
├─ ResourceUpdateService
└─ FluentGlassMainWindow
```

Each subsystem gets a narrow interface so one component can be replaced without changing unrelated deployment logic.

## 5. Portable Resource Model — Hybrid A

### 5.1 Embedded baseline

A complete baseline is included in the packaged application resources. PyInstaller may unpack these resources to its runtime extraction directory; that extraction is treated as read-only baseline content.

Baseline components:

```text
embedded/
├─ gse/
│  ├─ regular/x86/steam_api.dll
│  ├─ regular/x64/steam_api64.dll
│  ├─ experimental/x86/steam_api.dll
│  ├─ experimental/x86/steamclient.dll
│  ├─ experimental/x64/steam_api64.dll
│  ├─ experimental/x64/steamclient64.dll
│  ├─ steam_settings.EXAMPLE/
│  └─ tools/generate_interfaces/
├─ gse_coldclient/
│  ├─ ColdClientLoader.ini
│  ├─ steamclient.dll
│  ├─ steamclient64.dll
│  ├─ steamclient_loader_x86.exe
│  ├─ steamclient_loader_x64.exe
│  ├─ GameOverlayRenderer.dll
│  ├─ GameOverlayRenderer64.dll
│  └─ extra_dlls/
├─ gse_tools/
├─ migrate_gse/
│  ├─ migrate_gse.exe
│  └─ _internal/
├─ uc_online2/
│  ├─ x86/steam_api.dll
│  ├─ x64/steam_api64.dll
│  ├─ plugins/
│  └─ overlay-proxy assets when present in the verified UC release
├─ steamless/
│  ├─ Steamless.CLI.exe
│  └─ Plugins/
├─ rune_steamstub/
│  ├─ steamstub_x86.dll
│  └─ steamstub_x64.dll
└─ 7zip/7za.exe
```

The exact UC and Steamless release archive layouts are normalized by the build-time resource fetcher into the above runtime layout.

### 5.2 Portable updates

Updates are stored beside the application:

```text
<GSEAutoSetup.exe folder>/resources/
├─ state.json
└─ updates/
   ├─ .tmp/
   ├─ gse/
   ├─ gse_tools/
   ├─ gse_coldclient/
   ├─ migrate_gse/
   ├─ uc_online2/
   ├─ steamless/
   └─ rune_steamstub/
```

Resolution order:

1. verified portable update;
2. verified embedded baseline;
3. error with a repair/update action.

No component falls back to `%LOCALAPPDATA%` for package storage.

### 5.3 Update transaction

For every component:

1. download to `resources/updates/.tmp/<component>/`;
2. verify expected file set and available SHA-256/digest metadata;
3. unpack into a versioned staging directory;
4. run a component-specific validation probe;
5. atomically replace the active portable update directory;
6. update `resources/state.json`;
7. delete staging/temp data.

A failed update never invalidates the embedded baseline.

## 6. GSE Engine

### 6.1 Regular

Standard replacement deployment:

```text
steam_api64.dll      <- GSE Regular
steam_api64.dll.bak  <- original Steam API
steam_settings/
```

x86 uses the equivalent `steam_api.dll` names.

### 6.2 Experimental

Deploys the official Experimental API and matching experimental SteamClient where required. Built-in GSE overlay options are exposed in the UI.

### 6.3 ColdClient / steamclient_experimental

ColdClient is an official GSE deployment mode, not a custom “preserve API” approximation.

The packaged release contains:

```text
ColdClientLoader.ini
steamclient.dll
steamclient64.dll
steamclient_loader_x86.exe
steamclient_loader_x64.exe
GameOverlayRenderer.dll
GameOverlayRenderer64.dll
extra_dlls/steamclient_extra_x86.dll
extra_dlls/steamclient_extra_x64.dll
```

The tool generates `ColdClientLoader.ini` from the selected game executable, AppID, architecture, launch arguments, and advanced injection settings.

Default compatibility settings:

```ini
ForceInjectSteamClient=0
ForceInjectGameOverlayRenderer=0
IgnoreInjectionError=1
IgnoreLoaderArchDifference=0
```

`GameOverlayRenderer(64).dll` is enabled by default for ColdClient compatibility, because it is part of the official experimental ColdClient package. Force-injection remains Advanced and defaults off.

### 6.4 GSE connectivity presets

GSE only exposes:

- **Offline**: true offline mode.
- **LAN / emulator networking**: GSE networking enabled.

There is no GSE “Steam Online 480” preset. Steam Online belongs to UC Online2.

### 6.5 GSE overlay controls

When the selected GSE build supports overlay:

- Enable overlay
- Achievement notifications
- Achievement sounds
- Achievement icons
- Friend notifications
- FPS
- Frametime
- Playtime-related options where supported by the currently bundled GSE config
- Overlay hotkey

Unsupported options are disabled rather than written as invented config keys.

### 6.6 Official GSE config generation

The official `gse_fork_tools` output remains canonical. If it successfully creates `achievements.json`, `stats.json`, language data, controller data, etc., those files are preserved rather than normalized by the wrapper.

Steam Web API multilingual generation remains fallback only.

## 7. UC Online2 Engine

UC Online2 is a separate deployment engine.

### 7.1 Core deployment

Architecture determines which UC DLL is deployed:

```text
x86 -> steam_api.dll
x64 -> steam_api64.dll
```

Original Steam API is backed up adjacent to the replacement before deployment.

### 7.2 Configuration

The tool writes `union-crax.ini` next to the selected main game executable.

Default Spacewar profile:

```ini
[Settings]
AppId=480
ogAppId=<real game AppID entered in the UI>
PluginsFolder=plugins
GetStubbedLol=false
LoadOverlay=true
```

UI exposes:

- Spoof AppID, default `480`;
- Real/Original AppID (`ogAppId`), default current game AppID;
- Plugin folder;
- DLC list if the user chooses to configure it;
- Ticket emulation toggle where supported;
- SDR toggle where supported;
- Load Steam overlay;
- Overlay log;
- Overlay warning diagnostics;
- UC runtime SteamStub toggle when selected by DRM policy.

### 7.3 Backend detection

The tool scans the selected game directory for known runtime/backend markers and presents a recommendation, not an unconditional plugin drop.

Candidate categories derived from the supplied UC Online2 project:

- Steam only
- EOS
- Photon
- PlayFab
- coherence

The UI shows `Detected`, `Not detected`, or `Needs configuration`. Plugins are copied only when the user enables Auto Plugins or selects them explicitly.

### 7.4 Steam requirement

UC Online2 setup clearly indicates that Steam must be running for the normal Spacewar/custom AppID flow. The UI must not represent UC Online2 as an offline emulator.

## 8. SteamStub / DRM Coordinator

UI choice:

```text
Auto
Steamless
RUNE SteamStub Patcher
UC Runtime SteamStub
Disabled
```

`UC Runtime SteamStub` is only selectable for the UC Online2 engine.

### 8.1 Steamless

Flow:

```text
Game.exe
  -> Steamless.CLI.exe
  -> Game.exe.unpacked.exe
  -> backup original
  -> replace Game.exe with unpacked output
```

Adjacent original backup:

```text
Game.exe.bak
```

If `Game.exe.bak` already exists and is not tool-owned, use a conflict-safe name such as `Game.exe.gseauto.bak`; never overwrite it.

The handler captures stdout/stderr, validates that unpacked output exists and is a PE executable, and only then swaps files.

### 8.2 RUNE SteamStub Patcher

RUNE’s standalone patcher is a different mode:

```text
steamstub_x64.dll -> winmm.dll
```

or x86 equivalent.

Before deployment the tool checks for an existing `winmm.dll`. If it is not tool-owned, deployment requires explicit confirmation and a backup plan; Auto does not silently replace it.

### 8.3 UC Runtime SteamStub

For UC Online2, runtime SteamStub patching is configured through:

```ini
GetStubbedLol=true
```

No RUNE `winmm.dll` is required for this mode.

### 8.4 Auto policy

Auto is engine-aware:

**GSE**

1. detect likely SteamStub;
2. attempt Steamless if detected;
3. if Steamless succeeds, use unpacked EXE;
4. if it fails, recommend RUNE SteamStub Patcher and require user confirmation before proxy deployment;
5. if no SteamStub is detected, do nothing.

**UC Online2**

1. detect likely SteamStub;
2. attempt Steamless if detected;
3. if it fails, recommend UC Runtime SteamStub;
4. RUNE patcher remains an explicit alternate;
5. if no SteamStub is detected, do nothing.

Auto never creates `winmm.dll` merely because the toggle is enabled.

## 9. Backup and Restore

No game backup is stored in LocalAppData.

### 9.1 Adjacent first-class backups

Steam API:

```text
steam_api64.dll.bak
steam_api.dll.bak
```

EXE after Steamless:

```text
Game.exe.bak
```

### 9.2 Game-local transaction snapshot

Additional files are captured under:

```text
<Game>/.gse_auto_backup/
```

Manifest records:

- path;
- existed-before flag;
- hash before change;
- backup path;
- component/mode that changed it;
- tool ownership marker.

Restore returns:

- original Steam API;
- original EXE;
- previous `steam_settings`;
- previous `union-crax.ini`;
- previous UC/GSE plugin/proxy files;
- ColdClient files;
- SteamStub proxy files;
- tool markers.

A restore must not delete a pre-existing user/mod file that the manifest says existed before setup.

## 10. Save Management and migrate_gse

Save UI:

- **GSE Global** — `%APPDATA%\GSE Saves`;
- **Portable** — path relative to the game/GSE deployment;
- **Custom** — absolute user-selected path.

`migrate_gse` is exposed as a separate Tools action. It never runs automatically.

The bundled migration package includes its `_internal/` directory and is invoked only after a preview showing source and destination.

Supported use cases:

- old global Goldberg settings -> GSE format;
- old local `steam_settings`/`settings` -> GSE format;
- migration result copied to selected GSE destination after user confirmation.

The tool distinguishes **settings/config migration** from arbitrary game-save conversion.

## 11. LocalAppData Policy

V1.8 removes heavy package state from:

```text
%LOCALAPPDATA%\GSE Auto Setup\downloads
%LOCALAPPDATA%\GSE Auto Setup\packages
```

At startup, if these legacy tool-owned directories exist, the UI offers one-click cleanup. It does not delete unrelated user files.

Persistent application state lives beside the EXE:

```text
config.ini
resources/state.json
```

Steam Web API key remains protected with Windows DPAPI inside `config.ini`.

## 12. UI / UX Design

The main window uses a Windows 11 Fluent/Mica presentation when supported, with a dark acrylic-style fallback.

### 12.1 Main hierarchy

```text
GAME
  AppID | Folder | Main EXE | Architecture

ENGINE
  [ GSE ] [ UC ONLINE ]

GSE selected:
  [ Regular ] [ Experimental ] [ ColdClient ]
  Connectivity: [ Offline ] [ LAN ]

UC selected:
  Spoof AppID | Real AppID
  Backend detection
  Plugin controls
  Steam overlay controls

STEAMSTUB
  [ Auto v ] + detection/result status

OVERLAY & FEATURES
  context-sensitive controls

SAVES
  [ Global ] [ Portable ] [ Custom ]

ADVANCED
  injection, hotkeys, updater channel, diagnostics

RESOURCES
  component versions + update state

ACTIVITY
  collapsible structured log

BOTTOM ACTION BAR
  Restore | Dry Run | Setup
```

### 12.2 Visual behavior

- Mica backdrop through DWM when available.
- Semi-transparent cards and borders without excessive blur over text.
- 160–220 ms transitions for segmented selection, card expansion, progress, and Activity drawer.
- Reduced Motion option disables nonessential transitions.
- Layout targets approximately 1180–1280 px content width and remains usable on smaller screens without the V1.5 narrow-column regression.
- Primary actions remain visible in a sticky bottom action area.

### 12.3 Dry Run

V1.8 adds Dry Run. It produces a deployment plan listing files to create/replace/backup before any game files are changed.

This is particularly important for ColdClient, UC plugins, and proxy-DLL conflict handling.

## 13. Resource Update UI

Resource page lists independently:

- GSE stable package
- GSE tools
- GSE ColdClient package
- UC Online2
- Steamless
- RUNE SteamStub Patcher
- migrate_gse

Each row shows:

- active source: Embedded or Portable Update;
- version/tag when known;
- last check time;
- validation state;
- Update / Roll Back to Embedded actions.

Default update channel is **stable releases**. Advanced may expose a GSE development-channel option, but stable remains the default because GSE’s public release cadence differs from its dev-branch changelog.

## 14. Error Handling

- Component update failure -> retain currently valid resource and log exact reason.
- Missing embedded component -> resource health page shows Repair Needed.
- Backup failure -> abort before modifying target.
- Steamless produces no `.unpacked.exe` -> leave original untouched.
- Proxy conflict -> require explicit confirmation; never overwrite silently.
- Wrong architecture -> block deployment with detected/selected architecture details.
- UC without Steam running -> warn before launch/setup validation, but do not mislabel the deployment as broken if file generation itself succeeded.
- Official GSE generator timeout -> retain its partial work only if validation passes; otherwise fall back to multilingual Web API config generation.
- migrate_gse failure -> original settings remain untouched; migration output stays staged for inspection.

## 15. Testing Strategy

Implementation is test-first.

### Resource resolver

1. portable update wins over embedded baseline;
2. corrupt portable update falls back to embedded;
3. no heavy AppData cache is created;
4. temp update directories are removed after success/failure.

### GSE

5. Regular x64 creates adjacent original backup;
6. Experimental selects matching resources;
7. ColdClient package includes SteamClient loaders and both OverlayRenderer stubs;
8. generated ColdClient config has correct EXE/AppID/architecture;
9. force-injection defaults off;
10. official achievements output is preserved unchanged.

### UC Online2

11. x64/x86 API selection is correct;
12. `AppId=480` default and `ogAppId=<real AppID>` are written correctly;
13. backend detection does not copy unselected plugins;
14. Steam overlay options map only to known UC settings;
15. UC and GSE files are never deployed in the same transaction.

### SteamStub

16. Steamless backup-before-replace behavior;
17. Steamless failure leaves original EXE intact;
18. RUNE patcher maps architecture to the proper runtime DLL;
19. RUNE proxy conflict blocks silent overwrite;
20. UC runtime mode writes `GetStubbedLol=true` without RUNE proxy;
21. Auto with no SteamStub deploys no patcher;
22. Auto Steamless failure does not silently create `winmm.dll`.

### Restore

23. restore returns original Steam API hash;
24. restore returns original EXE hash;
25. restore preserves pre-existing third-party proxy files;
26. restore removes only tool-created UC/GSE files.

### Save/migration

27. global/portable/custom save config mapping;
28. migration is never automatic;
29. migration stages output before destination copy.

### UI/config

30. engine switching shows only relevant controls;
31. config.ini round-trips V1.8 settings;
32. DPAPI secret never appears as plaintext;
33. Reduced Motion disables nonessential animation;
34. Dry Run performs zero writes to game files.

### Release verification

35. pytest full suite passes;
36. compileall passes;
37. release ZIP contains no stale `dist`, build cache, pytest cache, or pyc files;
38. Windows build script validates presence of every required embedded baseline component before PyInstaller runs.

## 16. Implementation Boundaries

New/rewritten modules are expected to include:

```text
gse_autosetup/core/resources.py
gse_autosetup/core/resource_updates.py
gse_autosetup/core/deployment_plan.py
gse_autosetup/core/gse_engine.py
gse_autosetup/core/uc_engine.py
gse_autosetup/core/drm.py
gse_autosetup/core/migration.py
gse_autosetup/core/backup_restore.py
```

Existing `installer.py` will be reduced to orchestration or replaced by `DeploymentCoordinator`; it must not continue accumulating engine-specific branches.

UI code should split context panels into focused widgets rather than growing one giant `main_window.py`.

## 17. Success Criteria

V1.8 is complete only when:

- a clean Windows build can set up GSE Regular, Experimental, and ColdClient from embedded resources without internet;
- a clean Windows build can set up UC Online2 from embedded verified binaries without internet;
- Steamless and RUNE SteamStub modes are both available and distinct;
- UC runtime SteamStub is available only in UC mode;
- heavy package data no longer accumulates in LocalAppData;
- portable component updates can be installed independently without rebuilding the main EXE;
- Restore returns user/game files to their pre-transaction state;
- Dry Run accurately previews all modifications;
- all automated tests and Windows smoke tests pass.
