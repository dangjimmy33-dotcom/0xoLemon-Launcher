# Global launcher settings fix

Built on top of `src_GENERIC_UPDATE_FIXED.zip`.

## What changed

- Removed all selected-game diagnostics and install controls from Settings.
- Settings no longer renders the selected game's hero/header.
- Added a launcher-wide Settings page with standard sections:
  - General
  - Downloads & storage
  - Appearance
  - Launcher updates
- Added persistent launcher preferences using localStorage.
- Wired preferences to real behavior:
  - Startup page
  - Close button: exit or minimize
  - Confirm before uninstall
  - Default game library root
  - Open Downloads when a job starts
  - Pause an active download before launching a game
  - Reduce motion
  - Automatic launcher update checks
  - Manual launcher update check
- New installs now derive their default `common` and `downloading` folders from the configured library root.
- The Cache button opens the existing Cache page.
- `App_backup.tsx` was synchronized with the active implementation so it no longer contains the obsolete game-specific Settings page.

## Validation

- `tsc -b`: passed
- `vite build`: passed
- ZIP integrity: checked

No backend changes were required. Continue using `src-tauri_GENERIC_UPDATE_FIXED.zip`.
