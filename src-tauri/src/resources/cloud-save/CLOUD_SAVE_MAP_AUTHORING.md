# 0xoLemon Cloud Save Map — Authoring Guide

## Purpose

`cloud-save-map.builtin.json` is the authoritative map from a local launcher `gameId` to one or more Windows save locations. The Cloud Save engine does not guess paths and does not scan broad user folders.

Cloud Save applies only to games installed in **local mode**. Steam Lua/STFixer uses its separate flow.

## Per-game example

```json
{
  "my-game-id": {
    "enabled": true,
    "displayName": "My Game",
    "supportedInstallModes": ["local"],
    "roots": [
      {
        "id": "main-save",
        "label": "Game progress",
        "purpose": "save",
        "required": false,
        "matchPolicy": "allExisting",
        "candidates": [
          {
            "base": "KnownFolder.LocalAppData",
            "path": "Studio\\My Game\\Saved\\SaveGames",
            "priority": 100
          },
          {
            "base": "GameInstall",
            "path": "0xoLemon\\saves",
            "priority": 80
          }
        ],
        "include": ["**/*.sav", "**/*.profile"],
        "exclude": ["**/*.tmp", "**/*.log", "**/Cache/**"],
        "recursive": true,
        "followReparsePoints": false,
        "profileDiscovery": {
          "strategy": "all",
          "maxProfiles": 32,
          "maxDepth": 8
        },
        "restore": {
          "createMissingDirectories": true,
          "backupBeforeOverwrite": true,
          "atomicReplace": true
        }
      }
    ]
  }
}
```

## Supported bases

- `KnownFolder.LocalAppData`
- `KnownFolder.LocalAppDataLow`
- `KnownFolder.RoamingAppData`
- `KnownFolder.Documents`
- `KnownFolder.PublicDocuments`
- `KnownFolder.SavedGames`
- `KnownFolder.UserProfile`
- `GameInstall`
- `LauncherData`
- `AbsolutePath` only when explicitly allowed and binary-whitelisted

Known Folder values are resolved through Windows APIs, not string-concatenated from `%USERPROFILE%`.

## Path selection

- `allExisting`: sync every existing candidate; recommended for games with multiple profiles or migrated save paths.
- `firstExisting`: use the first existing candidate by priority.
- `highestPriority`: use the highest-priority existing candidate.
- If no optional candidate exists, the highest-priority candidate remains dormant so the launcher can detect the first save created later.

`exclude` always wins over `include`. Do not include graphics settings, shader caches, logs, crash reports, screenshots, or machine-specific configuration unless the game requires it for portable progress.

## Hard safety rules

A signed remote map still cannot override these binary-enforced limits:

- no `..` traversal;
- no broad drive/user-root selection;
- no junction, symlink, or reparse-point traversal;
- no scripts or executable actions;
- maximum 50,000 files, 20 GiB total, and 4 GiB per file;
- local install mode only.

Run a dry test on every supported game version before publishing a map.

## Signed repository

Build metadata and an immutable map target:

```powershell
python tools/cloud_save_repo.py build `
  --map resources/cloud-save/cloud-save-map.builtin.json `
  --schema resources/cloud-save/cloud-save-map.schema.json `
  --repo E:\cloud-save-repository `
  --keys E:\offline-cloud-save-keys `
  --rollout 10 `
  --rollout-seed cloud-map-2026-08
```

Verify before upload:

```powershell
python tools/cloud_save_repo.py verify --repo E:\cloud-save-repository
```

Host the public repository files over HTTPS. Set `OXO_CLOUD_SAVE_MAP_BASE_URL` at build time for production, or as a runtime environment variable during development. Copy the generated public `root.json` into launcher resources for the initial trust anchor. The authoring tool creates three offline root keys with a 2-of-3 threshold by default; keep them on separate protected media. Never upload, commit, or distribute the private key directory.

Recommended rollout: `1% → 10% → 25% → 50% → 100%`. Increment timestamp/snapshot/targets versions for each publication and never reuse a version number with different bytes. Root-key rotation is intentionally not automated in this first implementation; shipping a new trust root requires a reviewed launcher release.

## Tauri packaging

The final app must bundle all files under `resources/cloud-save/`. If they are omitted from the Tauri bundle, the engine cannot load the built-in fallback or trust root.
