# 007 First Light Smart Launcher

This project implements the launcher as a Steam-like depot system rather than an HDiffPatch wrapper.

## Runtime Model

- Version manifests describe every managed file by path, full-file SHA-256, size, and content-defined chunk references.
- FastCDC creates logical chunks with a target size of 1 MiB.
- Chunks are individually compressed and written into 256 MiB transport packs.
- The launcher caches logical chunks, not pack files, so interrupted downloads, repair, and rollback can resume precisely.
- Jobs are journaled to disk under the Tauri app data directory before each visible state change.
- File replacement is transactional: assemble to a temporary output, verify full-file hash, then replace the managed file.

## Current Depot State

| Version | Files | Packs (range)       | Latest |
|---------|-------|---------------------|--------|
| v1.0    | 73    | pack-00000..00200   | –      |
| v1.1    | 73    | (shared with v1.0)  | –      |
| v1.2    | 72    | pack-00201..00202   | ✓      |

All packs are hosted on Hugging Face: `CatManga/Cat-Manga` dataset, prefix `007-first-light`.

---

## Adding a New Version (e.g. v1.3)

### Step 1 — Compile the depot builder

```powershell
cd E:\007Launcher\src-tauri
cargo build --release --bin depot_builder
```

### Step 2 — Set HF token (only in current shell, never save to file)

```powershell
$env:HF_TOKEN = "hf_xxxxxxxxxxxxxxxxxxxx"
```

### Step 3 — Build and publish the new version

Use `build-version --extend-existing` to:
- Keep all existing versions in the catalog (v1.0, v1.1, v1.2 …)
- Re-use existing chunks (no re-upload of unchanged files)
- Start new packs from the next available pack index

```powershell
cd E:\007Launcher\src-tauri

.\target\release\depot_builder.exe build-version `
  --input        "E:\007 First Light"    `
  --version      v1.3                    `
  --out          "E:\007Launcher\depot\007-first-light" `
  --game-id      007-first-light         `
  --launch-executable "Retail\007FirstLight.exe" `
  --extend-existing                      `
  --upload-repo  "CatManga/Cat-Manga"    `
  --repo-type    dataset                 `
  --repo-prefix  "007-first-light"
```

> **What this does:**
> 1. Reads the local `catalog.json` to learn all existing chunk → pack mappings.
> 2. Walks `--input` directory, chunks every file.
> 3. Chunks already in a previous pack are **skipped** (no re-upload).
> 4. New chunks are written to new packs starting after the last existing pack index.
> 5. Each completed pack is uploaded to Hugging Face immediately, then deleted locally.
> 6. `catalog.json`, `versions/v1.3/manifest.json`, `manifests/v1.3.json`, and `versions/v1.3/build-info.json` are uploaded last.

### Step 4 — Update the launcher frontend

After publishing, open `E:\007Launcher\src\App.tsx` and update `fallbackCatalog`:

```ts
latestVersion: 'v1.3',
availableVersions: [
  { version: 'v1.0', label: 'Release Patch',  buildId: '2338871',  sizeBytes: 49_690_000_000, latest: false },
  { version: 'v1.1', label: 'Update 1.1',     buildId: '23531465', sizeBytes: 49_690_000_000, latest: false },
  { version: 'v1.2', label: 'Update 1.2',     buildId: '23600000', sizeBytes: 49_690_000_000, latest: false },
  { version: 'v1.3', label: 'Update 1.3',     buildId: '23700000', sizeBytes: 49_690_000_000, latest: true  },
],
```

Then build and release the launcher:

```powershell
cd E:\007Launcher\src-tauri
cargo tauri build
```

---

## Initial Build (for reference — first time from scratch)

Build a v1.0 + v1.1 depot pair from two local directories:

```powershell
cd E:\007Launcher\src-tauri
cargo run --bin depot_builder -- build-pair `
  --old-input  "E:\007 First Light - Sao chép" `
  --new-input  "E:\007 First Light"             `
  --out        "E:\007Launcher\depot\007-first-light" `
  --old-version v1.0 --new-version v1.1         `
  --launch-executable "Retail\007FirstLight.exe" `
  --upload-repo "CatManga/Cat-Manga"            `
  --repo-type dataset --repo-prefix "007-first-light"
```

The output layout is:

```
depot/007-first-light/
  catalog.json
  versions/
    v1.0/manifest.json
    v1.0/build-info.json
    v1.1/manifest.json
    v1.1/build-info.json
  manifests/
    v1.0.json
    v1.1.json
  packs/
    pack-00000.bin … pack-00200.bin   ← uploaded & deleted locally
```

---

## Proxy (Cloudflare Worker)

The Worker exposes only:

- `GET /catalog`
- `GET /manifests/{version}`
- `GET /packs/{packId}`

Configure secrets with Wrangler:

```powershell
cd E:\007Launcher\worker
npm install
npm run types
npx wrangler secret put HF_ORIGIN_BASE
npx wrangler secret put HF_BEARER_TOKEN
npm run deploy
```

`HF_ORIGIN_BASE` should point at the repo prefix, for example:

```
https://huggingface.co/datasets/CatManga/Cat-Manga/resolve/main/007-first-light
```

`HF_BEARER_TOKEN` is optional if the origin is public. The launcher must store only the proxy endpoint.

---

## Safety Rules

- Cache cleanup must delete only manifest-owned cache item paths.
- Uninstall and repair must preview affected managed files before changing them.
- No recursive deletion command or recursive cleanup flag is used by the project.
- Unknown files, saves, mods, and user data are preserved unless the user explicitly accepts a manifest-owned repair.
