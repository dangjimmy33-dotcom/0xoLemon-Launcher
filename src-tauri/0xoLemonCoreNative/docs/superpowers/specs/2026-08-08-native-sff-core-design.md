# Native SFF Core Integration Design

## Goal
Port the Python package `sff/core` (including `sff/core/storage`) into native C++ compiled into `0xoCore.dll`, without embedding CPython and without adding runtime files beyond the existing four DLL package.

## Scope
- In scope: core paths/utilities, process helpers, manifest integrity verification, machine-local secret storage, cache, VDF/ACF parsing and library discovery, INI option editing, settings persistence primitives, named-ID local registry, core enums/records needed by those services.
- Out of scope: `sff/gui`, `sff/ui`, `sff/webui`, network/provider/store code, cloud save UI, updater, download manager, DLC unlocker UI flows, Linux-specific code.
- No Python interpreter, `.py`, `.pyc`, `.pyd`, or Python runtime DLLs are shipped.

## Architecture
Create an internal CMake OBJECT library named `OxoSffNativeCore`. OBJECT output is linked directly into `OxoCore`, so no fifth DLL is produced. Native services live under `source/sffcore/` and use Windows/C++20 APIs plus libraries already linked by the project. A small `NativeCore` bootstrap is called from the existing orchestrator to initialize paths, load/cache settings state, and clean expired cache entries.

## Compatibility
The native port preserves behavioral intent rather than Python ABI. Data that is trivial and stable (VDF/ACF/INI/text files) stays compatible. Secret storage uses Windows DPAPI in the DLL and does not embed PyNaCl/CPython. Existing Python-only encrypted settings are not silently interpreted with a different cipher.

## Build/output constraints
`build.bat` must still install exactly:
- `0xoCore.dll`
- `0xoPayload.dll`
- `dwmapi.dll`
- `xinput1_4.dll`

Any extra DLL/file in `dist` remains a hard build failure.
