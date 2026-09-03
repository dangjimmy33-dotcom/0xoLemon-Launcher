# Savegame Manager + Google Drive

- Added a second top-level **Savegame Manager** workspace next to Setup & Emulator.
- Scans `%APPDATA%\\GSE Saves` and treats numeric child folders as Steam AppIDs.
- Resolves/caches Steam game names and header artwork by AppID without requiring the Steam Web API key.
- Local Backup archives the complete AppID folder and writes `.gse-save-manifest.json`.
- Restore creates a safety snapshot first and replaces the live AppID directory instead of merging files.
- Local backups live in `data\\save_backups\\<appid>` beside the application.
- Google Drive login uses desktop browser OAuth and `drive.file` scope only.
- Refresh tokens are stored only in `data\\google_oauth.bin`, protected by Windows DPAPI CurrentUser.
- Drive uploads are resumable and preserve a validated local ZIP snapshot.
- Added Backup All, per-game Backup to Drive, cloud history, and Restore from Drive.
- Personal `token.json` and `credentials.json` from the reference Drive Manager are not copied into this project.
- OAuth client lookup supports `client_secrets.json` beside the EXE or `resources\\google\\client_secrets.json`.
