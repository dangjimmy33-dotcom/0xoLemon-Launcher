# 0xoLemon Launcher

Tauri v2 + React launcher with a Steam-like depot pipeline:

- Content-defined chunks targeting about 1 MiB.
- Transport packs around 256 MiB to reduce request count.
- Manifest-driven install, update, verify, repair, rollback, cache, and resume.
- Hugging Face is used as storage, but launcher runtime reads only game depot metadata and packs.

## Run Dev

```powershell
cd E:\007Launcher
npm install
npm run tauri:dev
```

## Build Launcher

```powershell
cd E:\007Launcher\src-tauri
cargo tauri build
```

The portable executable is normally here:

```text
E:\007Launcher\src-tauri\target\release\first-light-smart-launcher.exe
```

## Publish A New Game Version

Use the wrapper tool instead of remembering the long `depot_builder` command.

### One-time build of the builder

```powershell
cd E:\007Launcher\src-tauri
cargo build --release --bin depot_builder
```

### Set HF token in the current shell

```powershell
$env:HF_TOKEN = "hf_xxxxxxxxxxxxxxxxxxxx"
$env:HF_HUB_ENABLE_HF_TRANSFER = "1"
```

Do not put the token in React, JSON, or committed config files.

### 007 First Light example

```powershell
cd E:\007Launcher
.\tools\publish_depot_version.ps1 `
  -GameId 007-first-light `
  -Version v1.3 `
  -Input "E:\007 First Light" `
  -LaunchExecutable "Retail\007FirstLight.exe"
```

### Among Us example

```powershell
cd E:\007Launcher
.\tools\publish_depot_version.ps1 `
  -GameId among-us `
  -Version v17.4I `
  -Input "E:\Compressed\Among Us" `
  -LaunchExecutable "Among Us.exe" `
  -RepoPrefix "among-us"
```

The tool does this:

1. Downloads only remote metadata into `E:\007Launcher\depot\<game-id>`.
2. Builds the new version with `--extend-existing`.
3. Reuses old chunk hashes from existing manifests.
4. Starts new pack IDs after the latest uploaded pack.
5. Uploads new packs and metadata to HF.
6. Verifies remote `catalog.json` points to the requested latest version.

No existing pack is overwritten.

## Build Asset Packs

Each game should have its own asset folder and cooked `.0xo` pack:

```text
E:\007Launcher\src\assets\007 first light
E:\007Launcher\src\assets\Among Us
E:\007Launcher\src-tauri\target\release\assets\games\<game-id>\*.0xo
```

Build asset packs:

```powershell
cd E:\007Launcher\src-tauri
cargo build --release --bin asset_pack_builder
.\target\release\asset_pack_builder.exe
```

After adding a new game, rebuild the launcher so bundled packs are included:

```powershell
cd E:\007Launcher
npm run tauri:build
```

## Manual Depot Builder Command

Use this only if the wrapper is not enough:

```powershell
cd E:\007Launcher\src-tauri
.\target\release\depot_builder.exe build-version `
  --input "E:\007 First Light" `
  --version v1.3 `
  --out "E:\007Launcher\depot\007-first-light" `
  --game-id 007-first-light `
  --launch-executable "Retail\007FirstLight.exe" `
  --extend-existing `
  --upload-repo "CatManga/Cat-Manga" `
  --repo-type dataset `
  --repo-prefix "007-first-light"
```

## Safety

Cleanup, repair, and uninstall flows must operate from manifest-owned file lists. Do not use recursive deletion commands or recursive cleanup flags.
