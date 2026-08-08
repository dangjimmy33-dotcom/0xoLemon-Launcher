# 0xoLemon CloudRedirect runtime/UI hotfix

## Scope
This hotfix addresses the five issues reproduced from the supplied Windows screenshots and current source snapshot:

1. CloudRedirect UI did not match the current 0xoLemon design language.
2. Steam client compatibility errors were presented as a generic failure.
3. Steam process state could remain stale after closing Steam.
4. Runtime deployment repeatedly failed with `Cannot sync ... Access is denied (os error 5)`.
5. Native 0xoLemon Cloud Save layout had markup/CSS selector mismatches and collapsed visually.

## Root causes and changes

### Runtime `Access is denied`
`atomic_copy()` copied a staged binary, reopened it with `File::open()` (read-only on Windows), then called `sync_all()`. Windows' `FlushFileBuffers` requires a write-capable handle, so this could return error 5. The staged file is now reopened with `OpenOptions::write(true)` before flushing.

### Steam process detection
The old detector searched human-readable `tasklist` output for the substring `steam.exe`. It now requests CSV output, requires an exact `steam.exe` image-name match, extracts PIDs, and exposes a lightweight Tauri command for polling. The React UI polls this state every 1.5 seconds and provides a `Close Steam` action that waits for the process to disappear.

### Steam compatibility
The downstream detector's version allow-list was one revision behind the bundled upstream source and has been synchronized with the vendor snapshot. The currently observed Steam build `1785799196` is still newer than the bundled CloudRedirect patch set. The launcher now blocks patching that unknown build explicitly rather than attempting unsafe bytes. Installing the engine itself remains separate from patching.

### CloudRedirect UI
The overview is flattened into the existing 0xoLemon graphite system: one title row, a thin tab rail, a single status strip, plain setup rows, restrained borders, and existing gold semantics. Decorative status-card density and gradient primary buttons were removed from the overview.

### Native 0xoLemon Cloud Save UI
The previous JSX rendered `<article>` metrics and `<button>` game rows while CSS targeted different element types. Hero markup also did not match the grid model. Selectors and markup now agree, the hero is compact, metrics are a single status strip, and installed games render as normal launcher rows.

## Verification performed in this environment
- Backend regression contract: PASS
- Existing CloudRedirect backend contract: PASS
- Frontend Cloud UI regression contract: PASS
- Existing CloudRedirect frontend contract: PASS
- TypeScript syntax/transpile check for modified TS/TSX: PASS
- Rust structural delimiter scan for modified Rust files: PASS

Full `cargo check`/Windows runtime verification could not be run in this Linux environment because Rust/MSVC/Steam are unavailable here.
