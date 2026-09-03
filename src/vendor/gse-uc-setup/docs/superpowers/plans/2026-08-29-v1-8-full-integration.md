# V1.8 Full Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build V1.8 with GSE Regular/Experimental/ColdClient and UC Online2 engines, Steamless/RUNE/UC SteamStub choices, hybrid embedded/portable resources, migrate_gse, portable updates, and a redesigned glass UI.

**Architecture:** Replace LocalAppData package caching with a `ResourceManager` that resolves per-component portable updates beside the executable before embedded resources. Deployment is split into independent GSE and UC Online2 engine modules; DRM handling is split into Steamless, RUNE proxy, and UC runtime modes. Existing game-local transactional restore remains the single rollback mechanism.

**Tech Stack:** Python 3.10+, PySide6, requests, PyInstaller, Windows executables/DLL resources.

**Spec:** `docs/superpowers/specs/2026-08-29-v1-8-full-integration-design.md`

## Global Constraints

- No large package/archive/extracted-component cache under `%LOCALAPPDATA%`.
- Resource priority is `resources/updates/<component>` beside the EXE, then embedded `resources/embedded/<component>`.
- Steam API and Steamless EXE originals are backed up beside the original file and never overwritten on reruns.
- Game-local `.gse_auto_backup` remains authoritative for full Restore.
- UC Online2 is a separate engine; do not merge GSE config into `union-crax.ini`.
- Auto SteamStub must not silently deploy a proxy DLL after Steamless failure.
- GSE official generator output remains canonical when available.

---

### Task 1: Portable Resource Manager and full embedded seed

**Files:**
- Create: `gse_autosetup/core/resources.py`
- Modify: `gse_autosetup/core/cache.py`
- Modify: `gse_autosetup/core/package.py`
- Test: `tests/test_resource_manager.py`

**Interfaces:**
- Produces: `ResourceManager.component_root(name) -> Path`, `update_root(name) -> Path`, `embedded_root(name) -> Path`, `temp_dir(name) -> context manager`.
- Existing `CacheManager` becomes metadata-only compatibility state and does not create `downloads` or `packages`.

- [ ] Write failing tests for update-over-embedded priority, portable temp cleanup, and no LocalAppData package directories.
- [ ] Run the tests and confirm RED.
- [ ] Implement `ResourceManager` and metadata-only `CacheManager`.
- [ ] Run tests and confirm GREEN.

### Task 2: Steamless and RUNE SteamStub strategies

**Files:**
- Create: `gse_autosetup/core/steamless.py`
- Modify: `gse_autosetup/core/steamstub.py`
- Test: `tests/test_steamless.py`
- Modify: `tests/test_steamstub.py`

**Interfaces:**
- Produces: `SteamlessManager.patch(exe_path, backup_file=None) -> Path` and portable-resource `SteamStubManager.deploy(...)`.
- Steamless writes `<exe>.unpacked.exe`, protects `<exe>.bak`, and promotes unpacked output only after success.

- [ ] Write failing tests for command construction, original EXE protection, failed-unpack no-op, and RUNE component portable resolution.
- [ ] Run RED.
- [ ] Implement managers.
- [ ] Run GREEN.

### Task 3: GSE ColdClient and migration resources

**Files:**
- Modify: `gse_autosetup/core/package.py`
- Create: `gse_autosetup/core/coldclient.py`
- Create: `gse_autosetup/core/migrate.py`
- Test: `tests/test_coldclient.py`
- Test: `tests/test_migrate.py`

**Interfaces:**
- Produces: `ColdClientInstaller.install(...) -> list[Path]`, `MigrateGSEManager.executable() -> Path`, `launch(...)`.

- [ ] Add failing tests for full ColdClient resource discovery and migration resource resolution.
- [ ] Run RED.
- [ ] Implement.
- [ ] Run GREEN.

### Task 4: UC Online2 engine

**Files:**
- Create: `gse_autosetup/core/uc_online.py`
- Test: `tests/test_uc_online.py`
- Modify: `gse_autosetup/core/models.py`

**Interfaces:**
- Produces: `UCOnlinePackage`, `UCOnlineInstaller.install(game_root, targets, main_exe, appid, spoof_appid, plugins, runtime_steamstub, backup_file) -> list[Path]`.
- Writes `union-crax.ini` beside the selected main EXE with `AppId`, `ogAppId`, `PluginsFolder`, and `GetStubbedLol`.

- [ ] Add failing tests for x86/x64 DLL selection, side-by-side backup, INI generation, and plugin copy.
- [ ] Run RED.
- [ ] Implement.
- [ ] Run GREEN.

### Task 5: Service orchestration and persisted settings

**Files:**
- Modify: `gse_autosetup/service.py`
- Modify: `gse_autosetup/core/tool_config.py`
- Modify: `gse_autosetup/core/models.py`
- Test: `tests/test_service_engines.py`
- Modify: `tests/test_tool_config.py`

**Interfaces:**
- `Inputs.engine`: `gse|uc`.
- `Inputs.gse_variant`: `regular|experimental|coldclient`.
- `Inputs.network_mode`: `offline|lan` for GSE.
- `Inputs.steamstub_mode`: `auto|steamless|rune|uc_runtime|disabled`.
- `Inputs.uc_spoof_appid`: integer, default 480.

- [ ] Add failing tests for configuration round-trip and engine dispatch.
- [ ] Run RED.
- [ ] Implement orchestration without silent RUNE fallback.
- [ ] Run GREEN.

### Task 6: Glass UI and resource controls

**Files:**
- Modify: `gse_autosetup/ui/main_window.py`
- Modify: `gse_autosetup/ui/theme.py`
- Test: `tests/test_ui_contract.py`

**Interfaces:**
- UI exposes Engine, GSE variant, network mode, SteamStub mode, UC spoof AppID, overlay feature switches, save routing, migration action, resource status, and Reduced Motion.

- [ ] Add static contract test for required controls/labels and absence of legacy `RUNE SteamStub runtime` toggle.
- [ ] Run RED.
- [ ] Implement responsive glass UI with animated sections and Mica fallback.
- [ ] Run GREEN where PySide6 is available; otherwise compile/static contract must pass.

### Task 7: Build/packaging, docs, and full verification

**Files:**
- Modify: `GSEAutoSetup.spec`
- Create: `BUILD_EXE_V1_8.bat`
- Create: `PATCH_NOTES_V1_8.md`
- Modify: `README.md`
- Modify: `README_VI.md`

- [ ] Bundle `resources/embedded/**` recursively in PyInstaller.
- [ ] Add build-time optional fetch hook for UC Online2 and RUNE SteamStub release assets into embedded resources when internet is available.
- [ ] Run `pytest -q`.
- [ ] Run `python -m compileall -q gse_autosetup`.
- [ ] Verify ZIP contains GSE full ColdClient, Steamless CLI/plugins, migrate_gse and updater metadata, and no caches/build output.
