# GSE / UC Setup V1.8.3

## Build fixes

- Fixed the Windows-only pytest failure `WinError 216`. The idle-timeout test previously created a text file named `generate_emu_config.exe`; POSIX can execute that through a shebang, but Windows `CreateProcess` correctly rejects it as a non-PE executable. The test now launches the real current Python interpreter and monkeypatches only the generator command, so it tests timeout behavior on Windows and POSIX.
- `BUILD_EXE.bat` no longer calls `FETCH_V18_RESOURCES.ps1` on every build. Existing `resources` are validated and used directly.
- Added `BUILD_EXE_FAST.bat` and `BUILD_EXE_DIRECT.bat`: no dependency installation and no resource downloads; they use the existing `.venv` and local resources.
- Added `UPDATE_RESOURCES.bat` as the explicit opt-in resource refresh path.
- Old versioned builder names redirect to the canonical V1.8.3 builder so they cannot accidentally trigger the old download path.
- The final build still copies `resources\embedded` and `resources\7zip` beside `dist\GSEAutoSetup.exe`.

## Resource rule

Normal/fast/direct builds never refresh GitHub components. If a required local component is missing, the build stops and tells the user to run `UPDATE_RESOURCES.bat` once.

## Verification

- 81 pytest tests passed.
- `compileall` passed.
