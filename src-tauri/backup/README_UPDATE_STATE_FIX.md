# Update state/version fix

Applied to the uploaded backend baseline `src-tauri(16).zip`.

## Root causes fixed

1. The update worker called `scanner::scan_install()` to determine the installed version. That scanner only recognizes the old hard-coded 007 First Light file-size signatures, so other games such as Hello Kitty Island Adventure always returned no version and failed during Scan.
2. The committed install marker trusted `manifest.version`. If a published manifest contained an old/stale internal version field, installing the selected latest catalog version could be recorded as the previous version.
3. Delta planning treated chunks listed by the old manifest as reusable without validating the actual bytes currently on disk. A stale marker/manifest could therefore lead to failed assembly or misleading zero-download plans.

## Changes

- Update planning and execution now resolve the installed base from:
  1. `.0xolemon/manifest.0xo`
  2. `.0xolemon/state.0xo`
  3. the old 007-only file signature scanner as a legacy fallback
- Normal games no longer depend on the 007-specific scanner.
- The selected version from `catalog.json` is canonical. Loaded manifests and the committed marker/installed manifest are written with that exact selected version.
- Reusable local chunks are verified with their BLAKE3 chunk hash before they are excluded from download planning. Invalid/missing local chunks are downloaded instead of causing a later assembly failure.
- The first update phases are now labelled `Read install state` and `Plan update`, so the UI no longer claims to be running a generic file scan.
- Existing pause/resume range files, retry logic, cancel cleanup, multi-repository logic, launch options, shortcuts, and install paths are preserved.

## Validation

- All JSON files parse successfully.
- Rust source delimiter/static lexical validation passed.
- `cargo check` was not available because the validation environment does not contain a Rust toolchain.
