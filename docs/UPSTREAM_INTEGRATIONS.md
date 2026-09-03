# Upstream Integration Policy

The launcher source tree remains authoritative. Upstream projects are reviewed for compatible fixes and optional capabilities; they are not copied wholesale over launcher-owned Rust, React, or native-core behavior.

## Tracked Sources

| Component | Upstream | Launcher integration |
| --- | --- | --- |
| CloudRedirect | `dangjimmy33-dotcom/CloudRedirect` | Built into the launcher and used by Cloud Saves. The vendored source is maintained against upstream releases. |
| SFF / SteaMidra | `dangjimmy33-dotcom/SFF` | Used as a feature and dependency reference. Exact-build downloads use DepotDownloaderMod on demand. Steam process handling stays launcher-owned. |
| LuaTools | `luatools/LuaTools-App` | Used as a compatibility reference for Lua, manifest, provider, and cloud-save workflows. Launcher source selection and native-core ownership remain authoritative. |
| BetterSteamTools | `luatools/BetterSteamTools` | Used as a native/runtime compatibility reference. External credential and ticket channels are not imported. |

## On-Demand Feature Packages

Optional binaries are installed under `feature-packages` beside the launcher executable. If a protected per-machine install directory is not writable, the package manager falls back to the current user's AppLocalData instead of weakening the ACL on `Program Files`. Packages are downloaded only after the user requests the relevant component. Downloads use an allowlisted HTTPS origin, bounded streaming, archive path validation, size limits, and SHA-256 verification.

Integration states exposed by Settings are intentionally distinct:

- `builtIn`: the launcher actively uses the implementation.
- `automatic`: the launcher installs and invokes the dependency for a defined workflow. Currently this applies to DepotDownloaderMod for exact BuildID switching.
- `dependency`: a runtime required by an automatic package. Currently this is the portable .NET 9 runtime.
- `component`: downloaded and verified, but not silently applied to a game. Per-game activation requires a separate transactional adapter.

The component catalog currently includes SmokeAPI, Uplay R1/R2 Unlocker, gbe_fork, and SteamAutoCrack. Downloading one of these components does not imply that it is active.

## Catalog Metadata

SFF's `store_metadata` snapshot is not bundled. The launcher already uses its live Steam catalog and provider-scoped Lua indexes, so shipping the snapshot would add tens of megabytes of duplicate data and would become stale independently of the live catalog.

## Safety Boundaries

- Existing Steam kill/restart behavior is not replaced by SFF process-management logic.
- Lua ownership, Live/Locked channels, manifest pinning, OnlineFix routing, and hot reload remain owned by `0xoLemonCoreNative`.
- Cloud save writes continue through CloudRedirect and the launcher's transactional save-map layer.
- Optional packages are never extracted into a game directory by the package manager.
- Package URLs, signed release URLs, tokens, and archive contents are not accepted from the frontend.
