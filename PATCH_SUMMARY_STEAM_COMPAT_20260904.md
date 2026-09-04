# 0xoLemon Steam compatibility + GitHub build fix (2026-09-04)

## Scope

Patched from the exact uploads:

- `src(20260903-181930).zip`
- `src-tauri(20260903-182027).zip`
- `0xoLemonCoreNative(4).zip`

`src/` required no production changes for these two bugs; it is repackaged unchanged for convenience.

## Root cause: Steam update diagnostics / Buy state

The native core already performs automatic SHA-256 detection and remote lookup, but that is **automatic detection/fetch**, not automatic synthesis of new Steam IPC/pattern metadata. The native IPC loaders use the exact `steamclient64.dll` SHA as the cache/remote key. When the per-build TOML is missing, the old bootstrap continued installing a partial hook set.

That partial state was unsafe: package/ownership/UI/IPC-dependent code could be present while the exact IPC metadata was missing. The diagnostic popup was also effectively enabled by default (`Settings.h` and the example config), so the failure was blocking the Steam startup flow.

### Changes

- Default native diagnostic popup is now **off**.
- Launcher hook installation migrates an existing `_0xolemoncore.toml` to `[boot].diagnostic_popup = false`.
- Unsupported Steam builds now use a **strict pass-through** path:
  - record SHA/build/status;
  - optionally write diagnostics/logs;
  - return before capture/package/UI/IPC/license/depot/routing mutation hooks are installed.
- SteamUI late retry is not started until steamclient compatibility has been confirmed.
- Compatible builds continue into the normal hook pipeline.

This intentionally does **not** fabricate ownership or force Steam's Buy button to Play. On an unsupported build the core leaves Steam's native entitlement/UI state untouched. If Steam itself considers a title owned, pass-through prevents the core from turning that native state into a broken partial state; if Steam considers it unowned, the native Buy state remains.

## Root cause: GitHub Actions `include_str!` failure

`src-tauri/src/lightning_integration.rs` compiled in:

- `../resources/lightning/data.json`
- `../resources/lightning/data-fix.json`
- `../resources/lightning/shop.json`

but those files are absent from the uploaded `src-tauri` tree. `include_str!` is compile-time, so this necessarily aborts compilation.

### Changes

`build.rs` now prepares three generated catalog files in Cargo `OUT_DIR`:

- if the real `resources/lightning/*.json` exists, its content is copied;
- if it is missing, valid empty JSON (`{}`) is generated and a Cargo warning is printed;
- `lightning_integration.rs` includes the generated `OUT_DIR` files.

This removes the hard compile failure without inventing catalog entries. Add the real JSON resources later if the feature needs actual catalog data.

## Warnings cleaned

- removed unused `SUPPORTED_STEAM_VERSIONS` import;
- removed unused `OsRng` / `RngCore` import.

## Files changed

### `src-tauri`

- `build.rs`
- `src/cloud_redirect/mod.rs`
- `src/depot_crypto.rs`
- `src/lightning_integration.rs`
- `src/open_steam_tool.rs`

### `0xoLemonCoreNative`

- `_0xolemoncore.toml.example`
- `source/config/Settings.h`
- `source/core/Orchestrator.cpp`
- `source/core/Orchestrator.h`
- `source/core/entry.cpp`

### `src`

- no production changes.

## Verification performed in the available environment

Fresh verification after the final strict pass-through change:

- `python -m pytest -q tests/steam_update_regression_test.py` -> **6 passed**
- `node --test lib/lightningIntegration.contract.test.mjs` -> **1 passed, 0 failed**
- `python src/runtime_contract_test.py` -> **passed**
- checked that the old `include_str!("../resources/lightning/...` paths are gone.

The current container does not provide `cargo`, `rustc`, or the Windows SDK, so a full Windows Rust/C++ build cannot be claimed here. GitHub Actions / a Windows build remains the definitive compiler validation.
