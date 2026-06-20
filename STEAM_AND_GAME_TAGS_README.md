# Steam integration and manual game tags

## Manual tag table

Edit this single file before building:

`src-tauri/game-tags.json`

Use the exact launcher `gameId` as the key. Supported built-in tag IDs:

- `denuvo` — red badge
- `online` — blue badge; enables the Steam-running recommendation and Spacewar 480 check
- `offline` — gray badge

Example:

```json
"my-game-id": ["denuvo", "online"]
```

Badges are rendered only over the library grid artwork. They are not added to the detail page.

## Non-Steam shortcut behavior

After install/update/repair commits successfully, the launcher adds or refreshes a non-Steam shortcut for the game in the most recently used Steam profile.

- When Steam is closed, `shortcuts.vdf` is updated immediately without starting Steam.
- When Steam is running, the operation is queued in the launcher's small AppData state file and applied after Steam closes or on the next launcher start.
- A backup named `shortcuts.vdf.0xolemon.bak` is written before replacement.
- Uninstall removes the desktop shortcut and removes, or queues removal of, the matching non-Steam shortcut.

## Spacewar 480 detection

The probe now reads Steam's installation path, parses `steamapps/libraryfolders.vdf`, scans every Steam library, and verifies `appmanifest_480.acf` plus its `installdir`. This covers Spacewar installed outside Steam's main folder.

## Chunk storage

No chunk-storage paths or cleanup rules were changed by this update.
