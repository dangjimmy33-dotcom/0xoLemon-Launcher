# Savegame Manager + Google Drive Design

## Status
Approved interaction/design direction: **A — each user connects their own Google Drive account through desktop OAuth**.

## Goal
Add a second top-level tab, **Savegame Manager**, to GSE / UC Setup. It manages GSE save folders keyed by Steam AppID, resolves game metadata/artwork, creates/restores full-folder backups, and can upload/download those backups to the user's own Google Drive account.

## Existing project integration
The current desktop app is PySide6 and builds its main UI in `gse_autosetup/ui/main_window.py`. The Savegame Manager must live inside the same process and use the same Fluent/Mica styling, reduced-motion setting, worker-thread conventions, and portable configuration model.

The existing `uploader-file-tool` provides reusable reference logic for:
- OAuth installed-app browser login (`InstalledAppFlow.run_local_server`)
- durable refresh-token lifecycle
- Windows DPAPI credential encryption
- Google Drive folder/file operations
- resumable upload/download progress

Only the minimal Drive/OAuth subset is ported. FastAPI, uvicorn, pywebview, sharing, retention, archive-sharing policy, and the old web UI are out of scope.

## Top-level navigation
The main window gets two persistent top-level tabs:

1. **Setup & Emulator** — current V1.8.4 functionality, behavior unchanged.
2. **Savegame Manager** — new save-management workspace.

Switching tabs does not rebuild the app state. Both tabs share the existing header/theme and save their UI preferences through the portable config store.

## Save sources
### Default source
The default source is:

`%APPDATA%\\GSE Saves`

Only direct child folders whose names are decimal integers are treated as game saves. `settings` and all other non-numeric folders are ignored.

Example:

- `%APPDATA%\\GSE Saves\\2050650` -> AppID `2050650`
- `%APPDATA%\\GSE Saves\\322170` -> AppID `322170`

### Additional sources
The UI supports:
- Global GSE Saves (default)
- portable/custom roots added by the user

Each source is a root whose numeric child folder is the AppID. Duplicate AppIDs across roots remain separate save entries because they may represent different save sets.

## Save entry model
Each detected save entry contains:
- AppID
- source root
- full save-folder path
- resolved game name
- header/capsule image path or URL cache key
- recursive folder size
- most recent modification time
- local backup count
- Drive backup count/status when connected

## Steam metadata and artwork
Game name and artwork are resolved from AppID.

Rules:
1. Prefer cached metadata first.
2. Resolve the game name from a public Steam metadata endpoint/API.
3. Resolve/store a Steam header/capsule image for the AppID.
4. Cache metadata/artwork in the app's portable data area so reopening Savegame Manager does not redownload unchanged assets.
5. Metadata failure never blocks save backup/restore; fallback display is `Steam App <appid>` with a placeholder image.

No Steam Web API key is required merely to list local saves.

## Savegame Manager UI
### Toolbar
- search field
- Refresh
- source selector
- grid/list toggle
- sort selector: Recently modified / Game name / AppID / Save size
- filter: All / Local only / Cloud available / Not backed up

### Game card/row
Displays:
- game header image
- game name
- AppID
- source label
- save size
- last modified timestamp
- cloud/local backup state

Actions:
- Open folder
- Backup
- Restore
- Backup to Drive
- Cloud history

### Cloud panel
When disconnected:
- `Connect Google Drive`

When connected:
- connected Google account identity if available
- `Backup selected`
- `Backup all`
- `Browse cloud backups`
- `Sync/refresh`
- `Disconnect`

Long operations report progress in the existing Activity/progress UI and run off the UI thread.

## Local backup format
A local backup archives the **entire AppID directory**.

Default portable backup root:

`<app directory>\\data\\save_backups\\<appid>\\`

Backup filename:

`YYYY-MM-DD_HH-mm-ss.zip`

Archive structure:

```text
2050650/
  <all original files and directories>
.gse-save-manifest.json
```

Manifest fields:
- format_version
- appid
- game_name
- source_root
- source_folder
- created_at UTC/local-offset timestamp
- total_uncompressed_bytes
- file_count
- content manifest/hash metadata sufficient for validation

The archive itself gets a SHA-256 after creation and that digest is stored in local backup metadata outside the ZIP. This avoids self-referential archive hashes.

## Backup correctness
Backup uses a temporary `.part` archive and atomically renames it only after ZIP validation succeeds.

Failure behavior:
- original save is never modified
- incomplete `.part` is removed or retained only as diagnosable temporary state
- no backup is listed as valid until integrity checks pass

## Restore behavior
Restore is destructive to the active AppID folder, so it is transactional:

1. Validate selected backup and manifest.
2. Verify AppID matches the target unless the user explicitly chooses another target.
3. Create an automatic safety backup of the current AppID folder.
4. Extract the selected backup to a temporary sibling folder.
5. Validate extracted structure.
6. Replace the target AppID folder atomically where possible; otherwise use rename/copy rollback-safe steps.
7. On failure, restore the pre-restore folder/safety backup.

The app must never partially merge a backup into the live save folder by default.

## Google OAuth
### Flow
Use OAuth 2.0 Installed App/Desktop flow:
- open system browser
- loopback callback on `127.0.0.1` and an ephemeral port
- request offline access so a refresh token can be retained

### Scope
Use:

`https://www.googleapis.com/auth/drive.file`

This intentionally avoids full `drive` scope.

Optional identity display may use `openid email` only if required to show the signed-in account email; Drive backup functionality must not require a broader Drive scope.

### OAuth client configuration lookup
Lookup order:
1. `<app directory>\\client_secrets.json`
2. `<app directory>\\resources\\google\\client_secrets.json`
3. packaged embedded resource if one is intentionally shipped

If no client secrets exist, the Drive panel remains usable for local backup and shows a clear `OAuth client configuration missing` status.

Personal `token.json` or credentials from the uploaded Drive Manager repository are **never bundled or reused**.

## Credential storage
Credential store:

`<app directory>\\data\\google_oauth.bin`

On Windows:
- encrypt credential JSON using DPAPI CurrentUser
- use app-specific entropy/namespace
- atomic file replacement
- never persist plaintext access/refresh tokens

Credential lifecycle:
- load encrypted credential
- refresh expired access token using refresh token
- persist refreshed credential immediately
- distinguish reauthorization-required errors from transient network failures
- Disconnect deletes the encrypted credential and in-memory service

A test-only cryptographic fallback may exist for non-Windows CI, but production Windows uses DPAPI.

## Google Drive layout
App-owned root folder:

`GSE Save Backups`

Per game folder:

`<appid> - <safe game name>`

Each timestamped backup ZIP is uploaded as a separate file. Drive metadata/appProperties include at minimum:
- app identifier for this application
- AppID
- backup timestamp
- format version
- SHA-256

Using `drive.file` means the app must primarily discover/manage files it created or files the user explicitly opened/authorized for the app.

## Drive uploads
Use Google Drive resumable upload.

Requirements:
- progress callback to UI
- configurable/appropriate chunk size aligned to Drive client constraints
- transient retry/backoff
- interruption must not mark the backup as uploaded
- upload local validated ZIP; do not stream directly from a changing live save folder

## Drive downloads and cloud restore
Cloud restore flow:
1. select cloud backup
2. download to portable temporary location
3. report progress
4. validate downloaded ZIP and expected digest when metadata is available
5. invoke the same local restore transaction

There is one restore implementation; cloud restore only adds a download stage.

## Dependency changes
Add only the client libraries required by the desktop Save Manager:
- `google-auth`
- `google-auth-oauthlib`
- `google-api-python-client`
- `cryptography` only if still required for test/non-Windows credential fallback

Do not import the old Drive Manager's FastAPI/uvicorn/web stack.

PyInstaller spec must collect the Google client library dependencies required at runtime and optionally the app's OAuth client JSON only when intentionally configured for distribution.

## New modules
Prefer focused modules under `gse_autosetup/save_manager/`:

- `models.py` — save entry, backup record, Drive backup record
- `scanner.py` — enumerate numeric AppID save folders and compute metadata
- `steam_metadata.py` — AppID name/artwork lookup + portable cache
- `backup.py` — ZIP creation, manifest, validation, local history
- `restore.py` — transactional restore + safety backup
- `oauth_store.py` — DPAPI durable Google credentials
- `google_auth.py` — installed-app authorization/refresh/disconnect
- `google_drive.py` — root/game folder resolution, resumable upload/download/listing
- `service.py` — orchestration API used by UI workers

UI additions may be split from the current large `main_window.py` into:
- `ui/setup_tab.py` only if needed to keep the existing behavior intact
- `ui/save_manager_tab.py` for the complete Save Manager surface

Avoid unrelated refactoring.

## Config additions
Portable `config.ini` stores non-secret Save Manager preferences only, such as:
- last selected top-level tab
- additional save roots
- view mode
- sort mode
- filter
- local backup root override

Google OAuth tokens never go in `config.ini`.

## Security and privacy
- no plaintext OAuth tokens
- no bundled user `token.json`
- no full-Drive OAuth scope
- local backup/restore works without Google account
- Drive network errors never block local save management
- game save data is uploaded only after explicit user backup-to-Drive action or explicit Backup All action

## Tests / acceptance criteria
1. Global scanner ignores `settings` and non-numeric folders.
2. Numeric AppID folder produces a save entry.
3. Duplicate AppID under different roots remains distinguishable.
4. Metadata cache is reused without network.
5. Missing metadata falls back to AppID name and does not block backup.
6. Local backup contains the complete AppID folder.
7. Backup manifest is created and valid.
8. Backup archive is only promoted after ZIP validation.
9. Restore creates a safety backup first.
10. Restore replaces rather than silently merges live save contents.
11. Failed restore rolls back to the previous live save.
12. Cloud restore reuses the same restore transaction after download.
13. OAuth uses `drive.file`, not full `drive` scope.
14. OAuth client search supports external client secrets beside EXE/resources.
15. Windows credential store uses DPAPI CurrentUser.
16. Plaintext token JSON is never written.
17. Expired credentials refresh and are persisted.
18. Invalid refresh token produces reauth-required state without deleting local saves.
19. Disconnect removes encrypted Google credential state.
20. Drive upload uses resumable mode and reports progress.
21. Interrupted upload is not marked successful.
22. Drive-created root/game folders are reused instead of duplicated.
23. Cloud list only presents backups owned/recognized by the application under the chosen Drive scope.
24. Save Manager remains fully usable while offline/disconnected from Drive.
25. UI has two persistent top-level tabs and tab selection persists.
26. Backup/restore/upload/download never block the PySide6 GUI thread.
27. PyInstaller smoke test imports Save Manager + Google client modules.
28. Existing Setup & Emulator regression suite remains green.

## Out of scope for this iteration
- arbitrary non-GSE game save-location discovery
- Steam Cloud synchronization
- automatic background backup without user opt-in
- cross-user sharing/public links
- Google Drive retention policies
- deduplication across different archives by block/chunk
- multi-provider cloud storage
