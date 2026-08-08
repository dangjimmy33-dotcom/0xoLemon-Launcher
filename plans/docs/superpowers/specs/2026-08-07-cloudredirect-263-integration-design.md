# CloudRedirect 2.6.3 Integration Design

## Goal
Integrate CloudRedirect 2.6.3 as the authoritative sync engine while presenting all supported operations inside the 0xoLemon React/Tauri UI.

## Scope
- Bundle/build the upstream Windows engine targets: `cloud_redirect.dll`, `cloud_redirect_cli.exe`, and `cloud760_tool.exe`.
- Keep 0xoLemon responsible for UI, i18n, notifications, confirmation, diagnostics, and launcher updates.
- Support CloudRedirect and STFixer modes.
- Support Google Drive, OneDrive, Cloudflare R2, generic S3-compatible providers, folder/mapped drive, and local-only mode.
- Support provider status/configuration, remote app inventory, per-app file inventory, manual sync, provider migration, orphan-blob cleanup, cloud data deletion, manifest publication/pinning diagnostics, stats inspection, and Cloud760 diagnostics.
- Do not embed or launch the upstream WPF UI or upstream updater/news UI.

## Architecture
The launcher ships the official engine source and builds the native targets before the Tauri bundle. Runtime binaries are copied to a versioned launcher engine directory and invoked by a typed Rust adapter. The adapter owns process execution, JSON parsing, safe config writes, secret storage, progress events, and destructive-operation guards. React consumes typed Tauri commands and renders a tabbed integrated interface.

## Safety
- Require Steam to be closed for patch/deploy/repair operations.
- Synchronize the remote app into the local cache and create a local safety archive before every destructive remote deletion.
- Serialize destructive operations with an operation lock.
- Never log secret values.
- Store R2/S3 credentials with Windows Credential Manager through the existing keyring crate; materialize a DPAPI-compatible credential file only for the engine.
- Preserve unknown keys when writing upstream config JSON.
- Make provider migration copy/verify first; switching provider is a separate commit after verification.

## UI
Use the existing launcher graphite/gold/blue palette, compact spacing, no glass/neon treatment. Tabs: Overview, Provider, Games, Backups, Migration, Maintenance, Diagnostics. All visible strings must exist in `en-US` and `vi-VN`.

## Verification
- Static contract tests for every command name and JSON field.
- TypeScript compiler syntax/type checks where project metadata is available.
- Rust lexical checks and `cargo check` when a toolchain is available.
- Upstream version and source-tree integrity checks.
- ZIP integrity checks.
