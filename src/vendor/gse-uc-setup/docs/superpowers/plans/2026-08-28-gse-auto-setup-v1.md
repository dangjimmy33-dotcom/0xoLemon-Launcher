# GSE Auto Setup V1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Windows PySide6 GUI that safely automates official GSE Regular setup from AppID, game folder, and Steam Web API key.

**Architecture:** The GUI delegates to focused backend services for release retrieval, Steam metadata, scanning, package extraction, config generation, installation, and restoration. Network credentials remain in memory only. A JSON manifest records every overwritten file for deterministic restore.

**Tech Stack:** Python 3.11+, PySide6, requests, py7zr, PyInstaller, pytest.

**Spec:** `docs/superpowers/specs/2026-08-28-gse-auto-setup-v1-design.md`

## Global Constraints
- Windows 10/11 target.
- Official source: `alex47exe/gse_fork`.
- Regular build by default.
- Steam Web API key must not be persisted.
- Always back up modified files.
- No non-Steam DRM removal.

---

### Task 1: Core models and release updater
**Files:** `gse_autosetup/core/models.py`, `gse_autosetup/core/github_client.py`, `tests/test_github_client.py`
**Interfaces:** Produces `ReleaseInfo`, `ReleaseAsset`, `select_windows_release_asset()`, `GitHubClient.latest_release()`.
- [ ] Write release-selection test.
- [ ] Verify it fails because core module is absent.
- [ ] Implement deterministic Windows release asset selection and GitHub metadata parsing.
- [ ] Run test to pass.

### Task 2: Game scanner and PE architecture
**Files:** `gse_autosetup/core/scanner.py`, `tests/test_scanner.py`
**Interfaces:** Produces `detect_pe_architecture(path)` and `scan_steam_api_targets(root)`.
- [ ] Test x86/x64 PE detection and redistributable exclusion.
- [ ] Verify red.
- [ ] Implement scanner.
- [ ] Verify green.

### Task 3: Steam schema conversion and settings builder
**Files:** `gse_autosetup/core/steam_api.py`, `gse_autosetup/core/config_builder.py`, `tests/test_config_builder.py`
**Interfaces:** Produces `SteamApiClient`, `schema_to_achievements()`, `schema_to_stats()`, `write_basic_settings()`.
- [ ] Test conversion from representative schema.
- [ ] Verify red.
- [ ] Implement conversion and file output.
- [ ] Verify green.

### Task 4: Package cache, install and restore
**Files:** `gse_autosetup/core/cache.py`, `gse_autosetup/core/package.py`, `gse_autosetup/core/installer.py`, `tests/test_installer.py`
**Interfaces:** Produces cache paths, package extraction, `Installer.install()` and `Installer.restore_latest()`.
- [ ] Test backup/restore primitives using temporary files.
- [ ] Verify red.
- [ ] Implement manifests, copies, interface generator invocation, rollback.
- [ ] Verify green.

### Task 5: Orchestrator and GUI
**Files:** `gse_autosetup/service.py`, `gse_autosetup/ui/main_window.py`, `gse_autosetup/ui/theme.py`, `app.py`
**Interfaces:** Produces `SetupService.run()` plus the PySide6 desktop entry point.
- [ ] Add service state/progress tests around pure validation helpers.
- [ ] Implement service orchestration.
- [ ] Implement Windows-style dark UI and worker thread.
- [ ] Syntax-check all Python files and run backend test suite.

### Task 6: Windows packaging and docs
**Files:** `requirements.txt`, `BUILD_EXE.bat`, `RUN_DEV.bat`, `README.md`, `GSEAutoSetup.spec`
**Interfaces:** Produces `dist\GSEAutoSetup.exe` on a Windows build host.
- [ ] Add pinned dependency ranges.
- [ ] Add one-click venv/build script.
- [ ] Document operation, safety/backup behavior, and limitations.
- [ ] Run compileall + pytest before packaging handoff.
