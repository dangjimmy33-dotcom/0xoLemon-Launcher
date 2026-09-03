# GSE / UC Setup V1.8.1

## Fixes

- Fixed PowerShell interpolation in `FETCH_V18_RESOURCES.ps1` (`${Name}: ...`).
- Added 4-attempt exponential retry for GitHub API calls and component downloads.
- Added `curl.exe` fallback when `Invoke-WebRequest` is reset by the remote host.
- Downloads now use `.part` files and are only promoted after a complete transfer/checksum verification.
- `gse_fork_tools` is now a required embedded baseline at build time; the build no longer silently produces an EXE without the official generator.
- Runtime update-check failure now falls back to the embedded/portable `gse_fork_tools` baseline instead of failing before trying it.

## Full GSE steam_settings

- Official generator now runs the canonical `_DEFAULT/1` complete GSE preset explicitly (`-def1 -clr -anon`).
- Removed unrelated CODEX/RUNE/Achievement-Watcher export switches from the GSE generation command.
- Official generated `steam_settings` is mirrored recursively, including dynamic runtime files such as `branches.json`, `depots.txt`, `supported_languages.txt`, controllers, images, inventory and other files the generator actually produces.
- Documentation-only Markdown/readme/license files are still excluded from game deployment.
- Added a post-copy mirror validator; if a generated runtime file disappears during deployment, setup rolls back instead of reporting success.
- Canonical static GSE defaults are seeded from the official `steam_settings.EXAMPLE` tree before user overrides: `configs.main.ini`, `configs.user.ini`, `configs.app.ini`, `configs.overlay.ini`, default controller files, fonts, sounds and default avatar assets.
- `configs.app.ini` is guaranteed to exist even in explicit limited Web API fallback mode.
- When Official GSE generator is enabled (default), generator failure no longer silently creates a sparse config. The setup stops with a clear error; users may explicitly disable the generator to request the limited fallback.

## Notes

`branches.json` and `depots.txt` are per-game dynamic data. V1.8.1 does not fabricate them from EXAMPLE files: they are kept when the official generator can obtain them from Steam.

## Packaging test builder alias fix
- Packaging tests now validate the canonical `BUILD_EXE.bat` instead of hard-coding the legacy `BUILD_EXE_V1_8.bat` filename.
- `BUILD_EXE_V1_8_1.bat` remains the versioned builder; the old V1.8 alias is no longer required for tests.
