# Launcher UI layout fix

Root cause: `launcher-update-banner` was inserted as the first direct child of the
CSS grid `.launcher-shell`. It occupied the first grid cell, pushed the sidebar
into the content column, and moved the workspace onto a new grid row.

Changes:
- Moved the launcher update banner outside `.launcher-shell`.
- Kept `.launcher-shell` limited to exactly two grid children: Sidebar + Workspace.
- Changed the shell flex sizing so the optional banner consumes its own height
  without overflowing the window.
- Added fallback banner/button colors when `--primary-accent` is undefined.

Validation:
- `npx tsc -b`: passed.
- `npm run lint`: passed.
