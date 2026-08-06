param(
  [Parameter(Mandatory = $true)]
  [string]$GameId,

  [Parameter(Mandatory = $true)]
  [string]$Version,          # e.g. "1.0.6"

  [Parameter(Mandatory = $true)]
  [string]$BuildId,          # e.g. "24424450"

  [Parameter(Mandatory = $true)]
  [string]$InputDir,         # folder containing update-only files

  [Parameter(Mandatory = $true)]
  [string]$LaunchExecutable, # e.g. "ACBlackFlag.exe"

  [Parameter(Mandatory = $true)]
  [string]$Repo,             # e.g. "CatManga/Cat-Manga" or "JOINCANE/0XoLemon"

  [string]$RepoType = "dataset",
  [string]$RepoPrefix = "",
  [string]$DepotRoot = "E:\007Launcher\depot",
  [string]$CargoManifest = "E:\007Launcher\src-tauri\Cargo.toml",
  [int]$PackTargetMb = 256,
  [int]$PackStartIndex = -1,   # -1 = auto-detect from catalog
  [string]$PackIdPrefix = "pack-",
  [string]$UploadDate = "",    # empty = today
  [switch]$NoEncryptPacks,
  [switch]$SkipUploadPrompt,
  [string[]]$Dependency = @()
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# ── Resolve defaults ──────────────────────────────────────────────────────

if ([string]::IsNullOrWhiteSpace($RepoPrefix)) { $RepoPrefix = $GameId }
if ([string]::IsNullOrWhiteSpace($UploadDate)) { $UploadDate = (Get-Date -Format "yyyy-MM-dd") }

$versionString = "$Version (Build $BuildId) - Uploaded $UploadDate"

$scriptRoot    = Split-Path -Parent $PSCommandPath
$syncTool      = Join-Path $scriptRoot "sync_hf_depot_metadata.py"
$depotOut      = Join-Path $DepotRoot $GameId
$srcTauri      = Split-Path -Parent $CargoManifest
$builder       = Join-Path $srcTauri "target\release\depot_builder.exe"

# ── Validate ──────────────────────────────────────────────────────────────

if (-not (Test-Path -LiteralPath $InputDir)) {
  throw "Input folder does not exist: $InputDir"
}

$exePath = Join-Path $InputDir $LaunchExecutable
if (-not (Test-Path -LiteralPath $exePath)) {
  throw "Launch executable not found: $exePath"
}

if ([string]::IsNullOrWhiteSpace($env:HF_TOKEN)) {
  throw "HF_TOKEN is not set. Set it before running this script."
}

if ((-not $NoEncryptPacks) -and [string]::IsNullOrWhiteSpace($env:OXO_DEPOT_KEY)) {
  throw "OXO_DEPOT_KEY is not set. Set it before running this script, or use -NoEncryptPacks."
}

if (-not (Test-Path -LiteralPath $syncTool)) {
  throw "Metadata sync tool not found: $syncTool"
}

if (-not (Test-Path -LiteralPath $CargoManifest)) {
  throw "Cargo manifest not found: $CargoManifest"
}

$updateFileCount = (Get-ChildItem -LiteralPath $InputDir -File -Recurse).Count

# ── Print plan ────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║           UPDATE-ONLY DEPOT BUILDER                        ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Game ID        : $GameId" -ForegroundColor Yellow
Write-Host "  Version        : $versionString" -ForegroundColor Yellow
Write-Host "  Input Dir      : $InputDir" -ForegroundColor White
Write-Host "  Input Files    : $updateFileCount" -ForegroundColor White
Write-Host "  Launch Exe     : $LaunchExecutable" -ForegroundColor White
Write-Host "  Repo           : $Repo ($RepoType)" -ForegroundColor White
Write-Host "  Repo Prefix    : $RepoPrefix" -ForegroundColor White
Write-Host "  Depot Output   : $depotOut" -ForegroundColor White
Write-Host "  Pack Target    : $PackTargetMb MiB" -ForegroundColor White
Write-Host "  Encrypt Packs  : $(-not $NoEncryptPacks)" -ForegroundColor White
if ($PackStartIndex -ge 0) {
  Write-Host "  Pack Start     : $PackStartIndex (manual)" -ForegroundColor White
} else {
  Write-Host "  Pack Start     : (auto-detect)" -ForegroundColor White
}
Write-Host ""

# ── Step 1: Sync metadata from HF ────────────────────────────────────────

Write-Host "━━━ Step 1/4: Syncing metadata from HuggingFace... ━━━" -ForegroundColor Cyan
$env:PYTHONUTF8 = "1"

New-Item -ItemType Directory -Force -Path $depotOut | Out-Null

python $syncTool `
  --repo $Repo `
  --repo-type $RepoType `
  --prefix $RepoPrefix `
  --out $depotOut

if ($LASTEXITCODE -ne 0) {
  throw "Metadata sync failed with exit code $LASTEXITCODE"
}

$catalogPath = Join-Path $depotOut "catalog.json"
if (-not (Test-Path -LiteralPath $catalogPath)) {
  throw "No catalog.json found after sync. Is the game ID / repo prefix correct?"
}

# ── Auto-detect pack start index & base version ──────────────────────────

$catalog = Get-Content $catalogPath | ConvertFrom-Json

$baseVersion = $null
foreach ($v in $catalog.versions) {
  if ($v.version -ne $versionString) {
    $baseVersion = $v.version
  }
}
if (-not $baseVersion) {
  # If only one version exists that's not ours, use it
  if ($catalog.versions.Count -eq 1) {
    $baseVersion = $catalog.versions[0].version
  }
}

Write-Host ""
Write-Host "  Base version   : $baseVersion" -ForegroundColor Green
Write-Host "  Base files     : $($catalog.versions | Where-Object { $_.version -eq $baseVersion } | Select-Object -ExpandProperty fileCount)" -ForegroundColor Green

if ($PackStartIndex -lt 0) {
  # Auto-detect: find max pack index + 1
  $maxIndex = -1
  foreach ($pack in $catalog.packs) {
    if ($pack.id -match 'pack-(\d+)') {
      $idx = [int]$Matches[1]
      if ($idx -gt $maxIndex) { $maxIndex = $idx }
    }
  }
  $PackStartIndex = $maxIndex + 1
  Write-Host "  Auto pack start: $PackStartIndex (last existing: pack-$('{0:D5}' -f $maxIndex))" -ForegroundColor Green
}
Write-Host ""

# ── Step 2: Build depot packs ─────────────────────────────────────────────

Write-Host "━━━ Step 2/4: Building depot packs... ━━━" -ForegroundColor Cyan

# Build builder if needed
if (-not (Test-Path -LiteralPath $builder)) {
  Write-Host "  Building depot_builder (release)..."
  Push-Location $srcTauri
  try {
    cargo build --release --manifest-path $CargoManifest --bin depot_builder
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
  } finally {
    Pop-Location
  }
} else {
  Write-Host "  Using existing builder: $builder"
}

$builderArgs = @(
  "build-version",
  "--input", $InputDir,
  "--version", $versionString,
  "--out", $depotOut,
  "--game-id", $GameId,
  "--pack-target-mb", ([string]$PackTargetMb),
  "--pack-id-prefix", $PackIdPrefix,
  "--pack-start-index", ([string]$PackStartIndex),
  "--extend-existing",
  "--launch-executable", $LaunchExecutable,
  "--keep-local-packs"
)

if ($NoEncryptPacks) {
  $builderArgs += "--no-encrypt-packs"
} else {
  $builderArgs += "--encrypt-packs"
}

foreach ($dep in $Dependency) {
  if (-not [string]::IsNullOrWhiteSpace($dep)) {
    $builderArgs += @("--dependency", $dep.Trim())
  }
}

& $builder @builderArgs
if ($LASTEXITCODE -ne 0) {
  throw "depot_builder failed with exit code $LASTEXITCODE"
}

Write-Host ""

# ── Step 3: Merge manifests ───────────────────────────────────────────────

Write-Host "━━━ Step 3/4: Merging update manifest with base... ━━━" -ForegroundColor Cyan

$mergeScript = @"
import json, os, sys

DEPOT      = sys.argv[1]
BASE_VER   = sys.argv[2]
UPDATE_VER = sys.argv[3]
BUILD_ID   = sys.argv[4]
GAME_ID    = sys.argv[5]

def load(p):
    with open(p, encoding="utf-8") as f: return json.load(f)
def save(p, obj):
    with open(p, "w", encoding="utf-8") as f: json.dump(obj, f, indent=2, ensure_ascii=False)
    print(f"  written: {p}")

base   = load(os.path.join(DEPOT, "versions", BASE_VER,   "manifest.json"))
update = load(os.path.join(DEPOT, "versions", UPDATE_VER, "manifest.json"))
bi_path  = os.path.join(DEPOT, "versions", UPDATE_VER, "build-info.json")
cat_path = os.path.join(DEPOT, "catalog.json")

base_files   = {f["path"]: f for f in base["files"]}
update_files = {f["path"]: f for f in update["files"]}

new_only  = [p for p in update_files if p not in base_files]
changed   = [p for p in update_files if p in base_files]
unchanged = [p for p in base_files   if p not in update_files]

print(f"  Base   : {len(base_files)} files | {base.get('totalSize',0):,} bytes")
print(f"  Update : {len(update_files)} files")
print(f"    New files     : {len(new_only)}")
print(f"    Changed files : {len(changed)}")
print(f"    Unchanged     : {len(unchanged)}")

merged      = {**base_files, **update_files}
merged_list = list(merged.values())
total_size  = sum(f["size"] for f in merged_list)
file_count  = len(merged_list)
chunk_count = sum(len(f.get("chunks", [])) for f in merged_list)

print(f"  Merged : {file_count} files | {total_size:,} bytes | {chunk_count} chunks")

# 1. manifest.json - update totalSize and files list
m = dict(update)
m["totalSize"] = total_size
m["files"]     = merged_list
m["rootLabel"] = f"{GAME_ID} {UPDATE_VER}"
save(os.path.join(DEPOT, "versions", UPDATE_VER, "manifest.json"), m)

# 2. build-info.json - fix stats
bi = load(bi_path) if os.path.exists(bi_path) else {}
bi.update({
    "buildId":    BUILD_ID,
    "version":    UPDATE_VER,
    "gameId":     GAME_ID,
    "fileCount":  file_count,
    "totalSize":  total_size,
    "chunkCount": chunk_count,
})
save(bi_path, bi)

# 3. catalog.json - fix stats for update version
cat = load(cat_path)
for ver in cat["versions"]:
    if ver["version"] == UPDATE_VER:
        ver["totalSize"]  = total_size
        ver["fileCount"]  = file_count
        ver["chunkCount"] = chunk_count
        break
save(cat_path, cat)

# Print verification
print(f"\n  === Verification ===")
cat2 = load(cat_path)
for v in cat2["versions"]:
    print(f"  {v['version']}: {v['fileCount']} files | {v['totalSize']:,} bytes | {v['chunkCount']} chunks")
print()
"@

$mergeScript | python - $depotOut $baseVersion $versionString $BuildId $GameId
if ($LASTEXITCODE -ne 0) {
  throw "Manifest merge failed with exit code $LASTEXITCODE"
}

# ── Step 4: Summary ──────────────────────────────────────────────────────

Write-Host "━━━ Step 4/4: Summary ━━━" -ForegroundColor Cyan
Write-Host ""

# List new packs
$newPacks = Get-ChildItem -LiteralPath (Join-Path $depotOut "packs") -Filter "*.bin" | Sort-Object Name
if ($newPacks.Count -gt 0) {
  Write-Host "  New packs ($($newPacks.Count)):" -ForegroundColor Green
  foreach ($p in $newPacks) {
    $sizeMB = [math]::Round($p.Length / 1MB, 1)
    Write-Host "    $($p.Name)  ($sizeMB MB)"
  }
} else {
  Write-Host "  WARNING: No pack files found!" -ForegroundColor Red
}

Write-Host ""
Write-Host "  Files to upload to $Repo :" -ForegroundColor Yellow
Write-Host "    1. catalog.json (updated)" -ForegroundColor White
Write-Host "    2. manifests/$versionString.json" -ForegroundColor White
Write-Host "    3. versions/$versionString/manifest.json" -ForegroundColor White
Write-Host "    4. versions/$versionString/build-info.json" -ForegroundColor White
foreach ($p in $newPacks) {
  Write-Host "    5. packs/$($p.Name)" -ForegroundColor White
}

Write-Host ""
Write-Host "╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Green
Write-Host "║  BUILD COMPLETE! Review files before uploading to HF.      ║" -ForegroundColor Green
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Green
Write-Host ""
Write-Host "  Depot path: $depotOut" -ForegroundColor White
Write-Host ""
