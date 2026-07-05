# Low Disk Space Depot Build Guide

## Problem

When building a depot for a large game (e.g., 132GB), you need:
- **Source files**: 132GB (game directory)
- **Depot output**: ~66GB (compressed packs + manifests)
- **Total needed**: ~198GB

If you only have 25GB free, the build will fail with "disk full" error.

---

## Solution: Incremental File Deletion

Use `--delete-source-after-pack` flag to delete source files **immediately after they're packed**.

### How It Works

1. Builder scans all files in `--input` directory
2. Files are sorted alphabetically by path
3. **For each file:**
   - Read → chunk → compress → encrypt → write to pack
   - **Delete source file immediately** ✅
4. When pack reaches target size (256MB), upload to Hugging Face
5. Delete local pack file after successful upload
6. Write catalog.json and manifest.json at the end

### Disk Usage During Build

With `--delete-source-after-pack`:
- **Start**: 132GB (source) + 0GB (depot)
- **Mid-build**: ~66GB (remaining source) + ~10GB (current pack)
- **End**: 0GB (source deleted) + ~0GB (packs uploaded & deleted)

**Net Result**: You only need ~15-20GB free space instead of 198GB!

---

## Step-by-Step Instructions

### 1. ⚠️ Backup Your Game First!

```powershell
# Copy game to external drive or another machine
Copy-Item -Path "E:\007 First Light" -Destination "F:\Backup\007 First Light" -Recurse
```

### 2. Set HF Token (Required for Upload)

```powershell
$env:HF_TOKEN = "hf_xxxxxxxxxxxxxxxxxxxx"
```

### 3. Compile Depot Builder

```powershell
cd E:\007Launcher\src-tauri
cargo build --release --bin depot_builder
```

### 4. Run Build with Incremental Deletion

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
  --repo-prefix  "007-first-light"       `
  --delete-source-after-pack             `
  2>&1 | Tee-Object -FilePath "E:\007Launcher\logs\depot-build-$(Get-Date -Format 'yyyyMMdd-HHmmss').log"
```

**What the flags do:**

| Flag | Purpose |
|------|---------|
| `--delete-source-after-pack` | **Delete source files after packing** (saves disk space) |
| `--extend-existing` | Keep existing versions in catalog (don't overwrite) |
| `--upload-repo` | Upload packs to Hugging Face immediately |
| `2>&1 \| Tee-Object` | Log all output to file (for debugging if it fails) |

### 5. Monitor Progress

You'll see logs like:

```
[DEPOT] WARNING: --delete-source-after-pack is enabled. Source files will be deleted as they are packed!
[DEPOT] pack target: 256 MiB | pack prefix: pack- | start index: 203
[DEPOT] Deleted source file: E:\007 First Light\Retail\007FirstLight.exe
[DEPOT] Deleted source file: E:\007 First Light\Retail\assets\textures\logo.png
...
[DEPOT] Uploading pack pack-00203.bin to CatManga/Cat-Manga
[DEPOT] Upload complete, deleting local pack
```

### 6. Verify Results

Check that catalog and manifests were written:

```powershell
Get-ChildItem "E:\007Launcher\depot\007-first-light" -Recurse
```

Expected output:

```
depot/007-first-light/
  catalog.json                  ← Updated with v1.3
  versions/v1.3/
    manifest.json               ← Contains all file entries
    build-info.json
  manifests/v1.3.json
  packs/                        ← Should be empty (all uploaded & deleted)
```

---

## Safety & Recovery

### What Happens If Build Fails?

**Scenario 1: Network error during upload**
- ❌ Some source files are deleted
- ✅ Packs uploaded before the error are safe on Hugging Face
- ✅ Unprocessed files remain in source directory
- **Recovery:** Re-run build with `--extend-existing` to continue from where it failed

**Scenario 2: Power loss / system crash**
- ❌ Source files processed before crash are deleted
- ✅ Catalog and manifests are written atomically at the end
- ❌ If crash happened before final write, catalog may be incomplete
- **Recovery:** Restore from backup and re-run build

**Scenario 3: Disk full during build**
- ❌ Some source files deleted
- ❌ Current pack may be incomplete
- **Recovery:** Free up space, restore backup, re-run

### Best Practices

1. ✅ **Always backup** before using `--delete-source-after-pack`
2. ✅ **Use logging** (`2>&1 | Tee-Object`) to track progress
3. ✅ **Test on small dataset first** (create a test folder with 1-2GB of files)
4. ✅ **Monitor disk space** during build: `Get-PSDrive E`
5. ✅ **Use `--extend-existing`** to resume if build fails mid-way

### Testing Before Production

Create a test build with a small subset:

```powershell
# Copy 1-2GB of files
New-Item -Path "E:\TestGame" -ItemType Directory
Copy-Item -Path "E:\007 First Light\Retail\*.exe" -Destination "E:\TestGame"

# Run test build with deletion
.\target\release\depot_builder.exe build-version `
  --input "E:\TestGame" `
  --version v0.0.1-test `
  --out "E:\TestOutput" `
  --game-id test-game `
  --delete-source-after-pack

# Verify E:\TestGame is empty and E:\TestOutput has manifest
```

---

## Disk Space Monitoring

During build, monitor disk usage:

```powershell
# Check free space every 30 seconds
while ($true) {
    Get-PSDrive E | Select-Object Name, Used, Free
    Start-Sleep -Seconds 30
}
```

Or use Windows Task Manager → Performance → E: drive.

---

## FAQ

**Q: Can I cancel the build mid-way?**  
A: Yes (Ctrl+C), but files already processed will be deleted. Restore from backup.

**Q: Will catalog.json still be correct if I use `--delete-source-after-pack`?**  
A: ✅ Yes! Catalog and manifests are written **after** all files are processed. File metadata is kept in memory during build.

**Q: What if I lose power during the build?**  
A: ❌ You'll lose processed source files. Restore from backup and re-run.

**Q: Can I build without uploading to HuggingFace?**  
A: Yes, remove `--upload-repo` flag. But then `--delete-source-after-pack` is **NOT recommended** because you'll have no backup!

**Q: How do I resume a failed build?**  
A: Use `--extend-existing` flag. The builder will skip chunks already in existing packs.

**Q: Will this affect existing versions (v1.0, v1.1, v1.2)?**  
A: ✅ No! With `--extend-existing`, old versions remain untouched in the catalog.

---

## Command Reference

```powershell
# Full build with all safety options
.\target\release\depot_builder.exe build-version `
  --input "E:\007 First Light" `
  --version v1.3 `
  --out "E:\007Launcher\depot\007-first-light" `
  --game-id 007-first-light `
  --launch-executable "Retail\007FirstLight.exe" `
  --extend-existing `
  --upload-repo "CatManga/Cat-Manga" `
  --repo-type dataset `
  --repo-prefix "007-first-light" `
  --delete-source-after-pack `
  --pack-target-mb 256 `
  --pack-prefix "pack-" `
  --format-version 1 `
  2>&1 | Tee-Object -FilePath "E:\007Launcher\logs\depot-build-$(Get-Date -Format 'yyyyMMdd-HHmmss').log"
```

### All Available Flags

| Flag | Description | Default |
|------|-------------|---------|
| `--input` | Source game directory | Required |
| `--version` | Version name (e.g., v1.3) | Required |
| `--out` | Output depot directory | Required |
| `--game-id` | Game identifier | Required |
| `--launch-executable` | Relative path to .exe | Optional |
| `--extend-existing` | Keep existing versions in catalog | false |
| `--upload-repo` | Hugging Face repo (user/repo) | Optional |
| `--repo-type` | Repo type (dataset/model) | "dataset" |
| `--repo-prefix` | Prefix path in repo | game-id |
| `--delete-source-after-pack` | **Delete source files after packing** | false |
| `--pack-target-mb` | Pack size in MB | 256 |
| `--pack-prefix` | Pack filename prefix | "pack-" |
| `--format-version` | Depot format version | 1 |
| `--encrypt-packs` | Enable transport encryption | true |
| `--no-encrypt-packs` | Disable encryption | false |
| `--encryption-key` | Custom encryption key (base64) | Env: OXO_DEPOT_KEY |
| `--keep-local-packs` | Don't delete packs after upload | false |

---

## Success Output Example

```json
{
  "gameId": "007-first-light",
  "outputDir": "E:\\007Launcher\\depot\\007-first-light",
  "catalogPath": "E:\\007Launcher\\depot\\007-first-light\\catalog.json",
  "versions": [
    {
      "version": "v1.3",
      "manifestPath": "versions/v1.3/manifest.json",
      "totalSize": 132450000000,
      "fileCount": 73,
      "chunkCount": 126234,
      "createdAt": "2026-07-04T15:23:45Z"
    }
  ],
  "packs": [
    { "id": "pack-00203", "path": "packs/pack-00203.bin", "size": 268435456, "sha256": "..." },
    { "id": "pack-00204", "path": "packs/pack-00204.bin", "size": 268435456, "sha256": "..." }
  ]
}
```

---

## Related Docs

- [ARCHITECTURE.md](./ARCHITECTURE.md) - Normal depot build workflow
- [DISCORD_ACCESS_SETUP.md](./DISCORD_ACCESS_SETUP.md) - Discord bot setup
