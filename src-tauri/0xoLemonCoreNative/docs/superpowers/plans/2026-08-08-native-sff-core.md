# Native SFF Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Python `sff/core` runtime dependency with native C++ services compiled directly into `0xoCore.dll` while preserving the exact four-DLL package.

**Architecture:** Add `source/sffcore` as an OBJECT library and link its object files into `OxoCore`. Implement Windows/C++20 equivalents for path/process/integrity/secret/cache/storage primitives; initialize them from the existing native orchestrator. Do not embed CPython or emit a separate native-core DLL.

**Tech Stack:** C++20, Win32, BCrypt/Crypt32, CMake OBJECT library, existing project logger/toml++.

## Global Constraints
- Windows x64 Release target.
- No UI code.
- No CPython, `.py`, `.pyc`, `.pyd`, or Python runtime DLLs.
- `dist` contains exactly four files: `0xoCore.dll`, `0xoPayload.dll`, `dwmapi.dll`, `xinput1_4.dll`.
- Internal SFF native core is linked into `0xoCore.dll`, not built as a fifth DLL.

---

### Task 1: Native core bootstrap, paths, and common types

**Files:**
- Create: `source/sffcore/NativeCore.h`
- Create: `source/sffcore/NativeCore.cpp`
- Create: `source/sffcore/CorePaths.h`
- Create: `source/sffcore/CorePaths.cpp`
- Create: `source/sffcore/CoreTypes.h`
- Modify: `source/CMakeLists.txt`
- Modify: `source/core/Orchestrator.cpp`

**Interfaces:**
- Produces: `SffCore::Initialize()`, `SffCore::Shutdown()`, `SffCore::Paths::{ModuleDirectory,DataDirectory,ManifestStagingDirectory}`.

- [ ] Add the `OxoSffNativeCore` OBJECT target and include it in `OxoCore`.
- [ ] Implement deterministic module/data path discovery without Python.
- [ ] Call `SffCore::Initialize()` from the existing startup path and `Shutdown()` during teardown.
- [ ] Configure with CMake and verify only the original four install targets exist.

### Task 2: Process and integrity primitives

**Files:**
- Create: `source/sffcore/CoreProcess.h`
- Create: `source/sffcore/CoreProcess.cpp`
- Create: `source/sffcore/CoreIntegrity.h`
- Create: `source/sffcore/CoreIntegrity.cpp`

**Interfaces:**
- Produces: process-running/elevation/Steam-launch/kill helpers and manifest size/magic/SHA-256/full-verification helpers.

- [ ] Port `sff/core/processes.py` behavior to Toolhelp32/ShellExecute/Explorer Win32 logic, excluding UI prompts.
- [ ] Port `sff/core/integrity.py` using BCrypt SHA-256 and binary manifest checks.
- [ ] Add a compile-only smoke translation unit check through the CMake target.

### Task 3: Secret storage and cache

**Files:**
- Create: `source/sffcore/CoreSecretStore.h`
- Create: `source/sffcore/CoreSecretStore.cpp`
- Create: `source/sffcore/CoreCache.h`
- Create: `source/sffcore/CoreCache.cpp`

**Interfaces:**
- Produces: `ProtectString/UnprotectString` using Windows DPAPI and an in-process TTL cache with optional atomic persistence.

- [ ] Implement machine/user-local DPAPI protection with no external runtime.
- [ ] Implement thread-safe TTL cache and atomic persistence under the native data directory.
- [ ] Run cache cleanup from `SffCore::Initialize()`.

### Task 4: VDF/ACF and INI storage

**Files:**
- Create: `source/sffcore/KeyValues.h`
- Create: `source/sffcore/KeyValues.cpp`
- Create: `source/sffcore/CoreAcf.h`
- Create: `source/sffcore/CoreAcf.cpp`
- Create: `source/sffcore/CoreIni.h`
- Create: `source/sffcore/CoreIni.cpp`

**Interfaces:**
- Produces: minimal Steam KeyValues parser/writer, library-folder discovery, ACF metadata parser, and comment-preserving-enough INI option replacement.

- [ ] Parse quoted KeyValues blocks and escaped strings used by Steam VDF/ACF.
- [ ] Discover valid Steam libraries from `config/libraryfolders.vdf`.
- [ ] Parse `appmanifest_<appid>.acf` and expose name/appid/state/install-dir/mounted depots.
- [ ] Implement in-place INI option replacement without Python ConfigUpdater.

### Task 5: Local settings/named IDs and final packaging audit

**Files:**
- Create: `source/sffcore/CoreSettings.h`
- Create: `source/sffcore/CoreSettings.cpp`
- Create: `source/sffcore/CoreNamedIds.h`
- Create: `source/sffcore/CoreNamedIds.cpp`
- Modify: `README.md`
- Modify: `build.bat`

**Interfaces:**
- Produces: native key/value settings store backed by TOML and a local ID→name registry without network lookup.

- [ ] Implement settings read/write through the already-linked toml++ library, with DPAPI helpers available for sensitive strings.
- [ ] Implement local named-ID registry by scanning saved Lua filenames; unknown entries fall back to the numeric ID because network lookup is out of scope.
- [ ] Add source audit to `build.bat` rejecting Python/runtime artifacts in `dist` and preserving exact-four-DLL verification.
- [ ] Verify CMake configure as far as the current host permits and run static source/output audits.
