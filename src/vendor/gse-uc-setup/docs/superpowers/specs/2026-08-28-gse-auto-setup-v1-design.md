# GSE Auto Setup V1 Design

## Goal
A Windows desktop utility that configures the official alex47exe GSE release for a selected game using only a Steam AppID, game folder, and Steam Web API key.

## Product constraints
- Windows 10/11 desktop application.
- PySide6 dark UI with Windows-like spacing, rounded cards, and blue accent controls.
- Prefer official release artifacts from `alex47exe/gse_fork`; do not silently install binaries from mirrors.
- Prefer Regular GSE. Experimental mode is optional and off by default.
- Never store the Steam Web API key on disk.
- Always back up original Steam API DLLs and generated/overwritten settings before modification.
- Provide one-click restore.
- Do not include or automate non-Steam DRM removal.

## Flow
1. Check GitHub latest release metadata and local cache.
2. Validate AppID and Web API key using Steam Web API; fetch public store metadata as supplemental information.
3. Optionally run the official `alex47exe/gse_fork_tools` generator with `-anon` for extended config, without prompting for Steam credentials.
4. Recursively scan the selected game folder for eligible `steam_api.dll` and `steam_api64.dll`, ignoring redistributable/cache/backup directories.
4. Download and cache the official Windows GSE release if required.
5. Extract package and locate Regular x86/x64 DLLs plus `generate_interfaces` tools.
6. Create a timestamped backup manifest and preserve original target DLLs/settings.
7. Run the matching official `generate_interfaces` executable against each original DLL and place the generated `steam_interfaces.txt` in local `steam_settings`.
8. Generate `steam_appid.txt`, achievements, stats, and minimal user config; download achievement icons when available.
9. Replace only the selected Steam API DLL targets with Regular GSE binaries.
10. Write setup manifest. Restore reverses the manifest.

## UI
Native window chrome with a dark Windows 11-inspired application surface. Header contains product title, official-source status, current cached GSE version, and updater state. Three primary fields are AppID, Game Folder, and Web API Key. A single blue `Setup GSE` button starts the workflow. Secondary actions are `Restore Original`, `Open steam_settings`, and an `Advanced` expander. Bottom area contains progress and a timestamped log.

## Error handling
Network failures may fall back to a previously validated cached GSE release. Invalid AppID/key does not modify the game. No eligible Steam API DLL results in a non-destructive failure. Any install exception triggers an attempted rollback from the current backup manifest.

## Testing
Unit tests cover release asset selection, PE architecture detection, scan exclusion/ranking, Steam schema conversion, and backup/restore primitives. GUI code is syntax-checked and backend tests are runnable without PySide6.
