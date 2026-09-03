# GSE / UC Setup V1.8.3

Windows GUI for two independent Steam integration engines: **GSE** (Regular / Experimental / ColdClient) and **UC Online2** (Spacewar/custom spoof AppID). V1.8.3 integrates Steamless, explicit RUNE SteamStub proxy deployment, UC runtime SteamStub, migrate_gse, portable component updates, game-local restore, and a Fluent/Mica UI.

See `README_VI.md` for the detailed Vietnamese guide and `PATCH_NOTES_V1_8_2.md` for the complete V1.8.3 change list.

## Build

Run `BUILD_EXE_V1_8_1.bat` on Windows. `FETCH_V18_RESOURCES.ps1` prepares the required official gse_fork_tools baseline with retry/curl fallback and refreshes the other component baselines before the one-file PyInstaller build. Runtime update overrides live beside the EXE under `resources/updates` rather than LocalAppData.

## V1.8.3 runtime resources
The Windows builder now copies the prepared `resources` baseline beside `dist/GSEAutoSetup.exe` and the runtime prefers that visible portable baseline before the one-file embedded fallback. Setup therefore does not re-download a valid local `gse_fork_tools` package.

## V1.8.3 build modes

- `BUILD_EXE.bat`: normal build, validates existing `resources\` and performs no runtime-resource refresh.
- `BUILD_EXE_FAST.bat` / `BUILD_EXE_DIRECT.bat`: fast/offline build using the existing `.venv` and resources; no package installation and no GitHub access.
- `UPDATE_RESOURCES.bat`: explicit resource refresh only.

Keep `dist\resources` beside `dist\GSEAutoSetup.exe`.
