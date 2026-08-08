# 0xoLemon × CloudRedirect 2.6.3 Integration Report

## Scope delivered

This integration replaces the partial/mock CloudRedirect V2 command surface with a typed Tauri adapter over the user-supplied upstream CloudRedirect engine source.

The launcher now owns:

- React UI and launcher-native visual system
- English/Vietnamese localization
- user confirmations and safety messaging
- process execution, diagnostics, and progress events
- launcher updates and release lifecycle

The upstream engine owns:

- Steam Cloud interception and redirection
- Google Drive, OneDrive, Cloudflare R2, generic S3, and folder providers
- remote inventories and file listings
- provider migration
- app synchronization
- manifest publication and blob garbage collection
- achievement/playtime metadata scanning
- Cloud760 tooling

The upstream WPF UI, its updater, and its news view are not launched or packaged as runtime resources.

## Source provenance

- User-supplied upstream archive: `CloudRedirect-master.zip`
- Archive SHA-256: `3d92bf74d3166aec2457456452f381734de5406d8f42f1b8fe733dd68b55b2ae`
- Version reported by `Version.props`: `2.6.3`
- Commit recorded in the supplied ZIP metadata: `9d0dbbf48f349a4172d2d47a936bb41c5f5ecff6`
- Official v2.6.3 release tag commit: `ecc84a0`

The supplied archive is a post-release master snapshot that still reports 2.6.3; it is not byte-identical to the official release tag.

The vendored tree is byte-identical to the supplied archive except for `0XOLEMON_INTEGRATION.md`.

## Backend architecture

### Versioned native runtime

`src-tauri/build-cloudredirect.ps1` builds:

- x64 `cloud_redirect.dll`
- x64 `cloud_redirect_cli.exe`
- Win32 `cloud760_tool.exe`

Artifacts are copied to:

`src-tauri/resources/cloud_redirect/engine/2.6.3/`

At runtime, the Rust adapter copies them transactionally into a versioned launcher data directory. Existing runtime files are moved to rollback names before commit.

### Typed Tauri adapter

New modules:

- `cloud_redirect_v2/models.rs`
- `cloud_redirect_v2/engine.rs`
- `cloud_redirect_v2/integration.rs`
- `cloud_redirect_v2/upstream_config.rs`
- `cloud_redirect_v2/backup.rs`
- `cloud_redirect_v2/manifest_pinning.rs`

The adapter exposes real commands for status, setup, provider configuration, inventories, synchronization, backup/restore, migration, maintenance, Cloud760, stats, and diagnostics.

Legacy mock sync commands are no longer registered with Tauri. OAuth compatibility commands remain registered because the integrated React UI uses them to create upstream-compatible DPAPI token files.

### Provider support

- Google Drive
- OneDrive
- Cloudflare R2
- Generic S3-compatible storage
- Folder / mapped drive
- Local-only mode

Folder configuration is written to both `sync_path` and the upstream `token_paths.folder`/`token_path` contract. This keeps the Steam DLL, CLI inventory, and migration tool on the same directory.

### Credential handling

OAuth and R2/S3 token files are DPAPI-encrypted using the current Windows user scope. The file format is compatible with the upstream Windows token store.

Secrets are not returned to the UI after saving. Forms expose only non-secret metadata and `hasSecret` state.

### Destructive-operation safety

Remote app deletion now requires:

1. Steam closed.
2. Exact `DELETE <AppID>` confirmation.
3. A fresh remote-to-local synchronization.
4. A committed local safety ZIP.
5. Only then, the upstream delete command.

The returned result includes the retained safety backup metadata.

Backup restore is journal-like and creates a pre-restore snapshot before replacing synchronized cache data.

### Provider migration

Migration uses the upstream streaming NDJSON CLI. Stdout and stderr are drained concurrently to avoid process deadlock. Progress is emitted through:

`cloudredirect://migration-progress`

The active provider changes only after the upstream migration exits successfully with zero failed files.

## Integrated UI

The Cloud Saves → STFixer/CloudRedirect branch now mounts the launcher-native CloudRedirect UI.

Tabs:

- Overview
- Provider
- Games
- Backups
- Migration
- Maintenance
- Diagnostics

The UI includes:

- engine and Steam/DLL health
- CloudRedirect/STFixer/third-party mode selection
- provider setup and connection tests
- Google Drive and OneDrive browser OAuth
- R2/S3 credential forms
- folder/mapped-drive support
- remote app and file inventory
- per-app and all-app sync
- local safety backups and restore
- provider migration progress
- manifest pinning
- stats metadata inspection
- orphan blob cleanup and full-manifest publication
- Cloud760 inventory/quota/delete tools
- diagnostics and technical log

Visible attribution identifies Selectively11 as the upstream project author. UI copy uses the existing graphite/gold/blue launcher palette and avoids the upstream WPF UI.

## i18n

- English CloudRedirect keys: 160
- Vietnamese CloudRedirect keys: 160
- Missing/mismatched keys: 0

## Upstream/runtime exclusions

Not integrated as runtime surfaces:

- WPF companion UI
- CloudRedirect updater
- CloudRedirect news view

The upstream source remains vendored for reproducible native builds, but Tauri bundles only the built engine runtime and version metadata.

## Verification performed

Passed in the available Linux container:

- frontend TypeScript focused type-check
- frontend command/provider/i18n contract test
- backend command/config/safety contract test
- upstream native `cloud_redirect` and `cloud_redirect_cli` CMake build
- Rust delimiter/attribute static scan
- upstream vendored-tree integrity check
- private-key material scan
- Tauri configuration JSON parse
- English/Vietnamese key parity
- original download/update module SHA-256 comparison
- ZIP CRC verification

The following download/update modules are byte-identical to the user's latest source:

- `job/direct.rs`
- `job/sequential.rs`
- `manifest.rs`
- `depot_crypto.rs`

## Verification not possible in this environment

The container does not contain Rust/Cargo, Windows MSVC, PowerShell, WebView2, or a Steam installation. Therefore these must still be run on the user's Windows build machine:

- `cargo check --lib`
- complete Tauri production build
- x64/Win32 MSVC native build
- OAuth browser callback test
- Steam DLL deployment and interception test
- live provider inventory/sync/migration test
- Cloud760 test with the required 32-bit `steam_api.dll`

## Licensing blocker before public distribution

No `LICENSE` file is present in the supplied upstream snapshot. Publicly bundling or distributing the upstream source or compiled binaries requires explicit permission from the CloudRedirect copyright holder unless a suitable license is later published.
