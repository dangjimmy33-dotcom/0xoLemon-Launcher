# Desktop shortcut game-folder fix

Built from the uploaded `src(11).zip` and `src-tauri(14).zip`.

## Fixed behavior

- The Windows desktop shortcut still launches 0xoLemon with the game's `--launch-game`, `--install-path`, and `--launch-executable` arguments.
- The per-game bootstrap executable is now stored in the actual installed game root as `0xoLemon Launcher.exe`.
- The shortcut's `WorkingDirectory` is the installed game root.
- Windows Explorer's **Open file location** therefore opens the real game directory instead of `%AppData%\\Roaming\\0xoLemon`.
- Legacy `%AppData%\\0xoLemon\\0xoLemon-<game-id>.exe` bootstrap copies are removed after migration.
- Existing registered shortcuts are migrated automatically when the main launcher starts.
- Uninstall removes both the desktop shortcut and the in-game/legacy bootstrap executables.

No frontend behavior, game files, depot/chunk logic, update version picker, or Steam non-Steam integration was changed.
