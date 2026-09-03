# 0xoLemon GSE / Depot integration patch — 2026-09-03

## GSE achievements / localization
- Removed default `-skip_ach` from the Rust fallback official generator invocation.
- Changed bundled Python original-core setup to call the official generator with `skip_achievements=False`.
- Canonical generator `achievements.json` and `stats.json` now win and are preserved byte-for-byte by the fallback writer.
- If the generator already produced achievements, Steam Web API localization fallback is skipped entirely.
- If canonical achievements are absent, Rust fallback can fetch/merge multiple Steam languages instead of English only.
- `supported_languages.txt` is preserved when already generated.
- Canonical defaults no longer deploy README/LICENSE/CHANGELOG documentation files as runtime assets.

## GSE UI parity
- Restored `Setup & Emulator` label.
- Centered the GSE workspace/header to the audited 1240px-style layout.

## DepotDownloader version switching / resume
- Legacy `steam.rs` download path no longer passes `-manifestfile` to DepotDownloaderMod.
- It seeds the native manifest cache first, then launches the target manifest with `-verify-all`.
- Existing DepotDownloader backend keeps BuildID manifest cache/version switching and stop→restart resume behavior.

## Research integrated
- `src-tauri/docs/research/depotdownloader-audit-2026-09-03.md`
- `src-tauri/docs/research/gse-parity-audit-2026-09-03.md`
- `src-tauri/docs/research/INTEGRATION_STATUS.md`

## Verification performed in this environment
- Frontend contract tests: 18/18 passed.
- GSE original-core protocol contract: passed.
- GSE original-core parity contract: passed.
- Targeted generator/localization/full-config tests: 12/12 passed.
- Rust `cargo check` was not available because Cargo/Rust is not installed in this container.
