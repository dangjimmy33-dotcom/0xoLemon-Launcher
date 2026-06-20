# Geometry Dash Play State Fix

Fixes installed-state detection after Geometry Dash download.

Root cause: old install markers could store the manifest/display id (`Geometry-Dash` / `Geometry Dash`) while the launcher expects canonical id `geometry-dash`. `get_game_install_state()` then filtered the marker out and showed `not installed` instead of `Play`.

Changes:
- `write_install_marker()` now always writes `source.game_id` as canonical `gameId`.
- `game_install_state()` accepts and sanitizes older marker ids, then rewrites the sanitized marker automatically.
- Existing already-downloaded Geometry Dash installs should become recognized after launching the patched build once.

After applying, build:

```powershell
cd E:\007Launcher
npm run tauri build
```

Quick check after running the patched launcher once:

```powershell
Get-Content "E:\0xoLemon store\common\Geometry Dash\.0xolemon\state.0xo"
```

Expected `gameId`:

```json
"gameId": "geometry-dash"
```
