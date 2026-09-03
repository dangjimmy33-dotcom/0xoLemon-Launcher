# GSE Auto Setup V1.6 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship V1.6 with adjacent Steam API backups, two deployment modes, optional RUNE SteamStub runtime deployment, portable DPAPI-backed config.ini persistence, filtered config deployment, and a responsive Fluent/Mica UI.

**Architecture:** Keep the existing GSE setup pipeline as Mode A and add a separate preserve-original loader pipeline as Mode B. Persist tool state next to the executable, keep reversible file operations in the installer manifest, and treat SteamStub as an optional runtime stage sourced from the RUNE component release with SHA-256 verification. UI is a thin PySide6 client over the service/installer layer.

**Tech Stack:** Python 3.10-3.14, PySide6, requests, PyInstaller, Windows DPAPI via ctypes, native 7-Zip.

**Spec:** `GSE_Auto_Setup_V1_6_DESIGN.md`

## Global Constraints
- Adjacent backup must be `steam_api64.dll.bak` / `steam_api.dll.bak` next to the replaced API and must never overwrite an existing valid backup.
- Mode B must preserve the original Steam API bytes.
- Steam Web API key is persisted only as a DPAPI-protected blob in `config.ini` beside the EXE.
- Markdown/docs are never deployed into the game folder.
- SteamStub stage is independently toggleable, checks latest RUNE component release, verifies SHA-256, and never silently overwrites an existing unrelated `winmm.dll`.
- Overlay requires Experimental GSE.

---

### Task 1: Portable config and DPAPI
**Files:** Create `gse_autosetup/core/tool_config.py`; modify UI to consume it; tests in `tests/test_tool_config.py`.
- [ ] Add failing persistence tests.
- [ ] Implement config.ini load/save and DPAPI secret helpers.
- [ ] Verify tests.

### Task 2: Adjacent backup and filtered settings deployment
**Files:** Modify `core/installer.py`, `core/official_generator.py`; tests in `tests/test_installer.py` and `tests/test_official_generator.py`.
- [ ] Add failing adjacent-backup/no-doc-copy tests.
- [ ] Implement idempotent `.bak` creation and doc filtering.
- [ ] Verify tests.

### Task 3: Preserve-original deployment mode
**Files:** Create `core/preserve_loader.py`; modify `service.py`, `models.py`, spec/resources; tests in `tests/test_preserve_loader.py`.
- [ ] Add tests proving original Steam API hash remains unchanged.
- [ ] Bundle user-provided loader seed resources.
- [ ] Implement x64 loader deployment and reversible backup list.
- [ ] Verify tests.

### Task 4: RUNE SteamStub runtime component
**Files:** Create `core/steamstub.py`; modify `service.py`; tests in `tests/test_steamstub.py`.
- [ ] Add release parsing/hash/conflict tests.
- [ ] Implement latest-release download and verified extraction.
- [ ] Deploy as `winmm.dll` next to selected game EXE only when enabled and no conflict exists.
- [ ] Verify tests.

### Task 5: Fluent/Mica UI and deployment mode UX
**Files:** Replace `ui/main_window.py`, refine `ui/theme.py`.
- [ ] Fix width bug and responsive layout.
- [ ] Add deployment-mode cards, animated toggles, config persistence, inline backup state, and Activity drawer.
- [ ] Apply Mica/backdrop and soft animations.
- [ ] Verify compile and smoke-import.

### Task 6: Packaging and regression suite
**Files:** Modify `GSEAutoSetup.spec`, `BUILD_EXE.bat`, docs/version files.
- [ ] Bundle preserve-loader resources.
- [ ] Run full pytest suite.
- [ ] Run compileall.
- [ ] Package V1.6 source ZIP and verification manifest.
