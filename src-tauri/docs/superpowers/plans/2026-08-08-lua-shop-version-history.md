# Lua Shop Version History Implementation Plan

**Goal:** Add an automatic BuildID/date dropdown to every Lua Shop add flow, seeded from SteamDB Patchnotes RSS and merged with the launcher's primary custom manifest source.

**Architecture:** The Tauri backend owns network access and merges two views: installable builds from the custom source, and historical metadata from SteamDB RSS. RSS-only rows remain visible but are marked unavailable until a configured source can resolve exact depot manifests. The frontend opens one shared version picker before adding a game and passes the chosen BuildID to the version-aware install command.

**Tech Stack:** Rust/Tauri, reqwest blocking client, std-only RSS parser, React/TypeScript.

## Global Constraints
- SteamDB RSS is metadata/history only; do not poll it for realtime patch detection.
- Custom manifest source remains primary.
- Never claim an exact historical version is installable unless depot manifest mappings are available.
- Keep existing OpenSteamTool manifest-code runtime fallback only for already-known manifest GIDs.
- No additional runtime process or UI application.

### Task 1: Pure RSS parser
- Add `src/shop_lua_versions.rs` with tests for BuildID, title, and pubDate extraction.
- Verify using `rustc --test` independently from Tauri.

### Task 2: Backend merge and cache
- Extend `BuildInfo` with patch title, installability, and source metadata.
- Cache RSS by AppID with TTL and merge into custom build results.
- Preserve SteamCMD current-branch date as a secondary date source.
- Resolve the canonical custom-source game folder by AppID rather than trusting display name.

### Task 3: Version-aware Lua Shop UI
- Open the shared build picker for ordinary Lua Shop cards instead of immediate add.
- Show BuildID + date + version/title.
- Disable metadata-only rows and explain why they cannot be selected.
- Pass selected exact build to `lua_shop_install_game`.

### Task 4: Source fallback semantics
- Preserve custom source as primary for manifest binaries and configured keys.
- Preserve 0xoLemon/OpenSteamTool manifest request-code fallback only after a manifest GID is known.
- Surface a clear error when RSS has a BuildID but no source can map it to exact depot manifests.

### Task 5: Verification and packaging
- Run pure Rust parser tests.
- Run source-contract checks for command wiring and dropdown behavior.
- Zip modified frontend and Tauri source separately.
