param(
  [Parameter(Mandatory = $true)]
  [string]$GameId,

  [Parameter(Mandatory = $true)]
  [string]$Version,

  [Parameter(Mandatory = $true)]
  [string]$InputDir,

  [string]$LaunchExecutable = "",
  [string]$LaunchOptionsJson = "",
  [string]$Repo = "CatManga/Cat-Manga",
  [string]$RepoType = "dataset",
  [string]$RepoPrefix = "",
  [string]$DepotRoot = "E:\007Launcher\depot",
  [string]$CargoManifest = "E:\007Launcher\src-tauri\Cargo.toml",
  [string]$SyncToolPath = "",
  [string]$EncryptionKey = "",
  [switch]$KeepLocalPacks,
  [switch]$NoEncryptPacks,
  [switch]$ForceRebuild,
  [switch]$NoSyncMetadata,
  [switch]$NoExtendExisting,
  [switch]$UseBuilderUpload,
  [switch]$UploadOnly,
  [switch]$DeleteSourceAfterPack,
  [switch]$UploadPacksIncrementally,
  [int]$PackTargetMb = 256,
  [int]$PackStartIndex = 0,
  [string]$PackIdPrefix = "pack-"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Test-ZstdMagicAtStart([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path)) { return $false }
  $fs = [IO.File]::OpenRead($Path)
  try {
    $b = New-Object byte[] 4
    $n = $fs.Read($b, 0, 4)
    return ($n -eq 4 -and $b[0] -eq 0x28 -and $b[1] -eq 0xB5 -and $b[2] -eq 0x2F -and $b[3] -eq 0xFD)
  } finally {
    $fs.Close()
  }
}

if ([string]::IsNullOrWhiteSpace($RepoPrefix)) {
  $RepoPrefix = $GameId
}

$scriptRoot = Split-Path -Parent $PSCommandPath
$incrementalUploadTool = Join-Path $scriptRoot "hf_incremental_upload_depot.py"
if ([string]::IsNullOrWhiteSpace($SyncToolPath)) {
  $SyncToolPath = Join-Path $scriptRoot "sync_hf_depot_metadata.py"
}

if (-not $UploadOnly) {
  if (-not (Test-Path -LiteralPath $InputDir)) {
    throw "Input folder does not exist: $InputDir"
  }

  if (-not [string]::IsNullOrWhiteSpace($LaunchExecutable)) {
    $exePath = Join-Path $InputDir $LaunchExecutable
    if (-not (Test-Path -LiteralPath $exePath)) {
      throw "Launch executable was not found: $exePath"
    }
  }
}

if ([string]::IsNullOrWhiteSpace($env:HF_TOKEN)) {
  throw "HF_TOKEN is not set. The local GUI backend should pass it to this child process."
}

# The local GUI normally passes OXO_DEPOT_KEY through the child-process environment.
# This parameter exists only as a fallback for direct manual PowerShell usage.
if (-not [string]::IsNullOrWhiteSpace($EncryptionKey)) {
  $env:OXO_DEPOT_KEY = $EncryptionKey
}

# Make Hugging Face uploads less spammy and safe for normal Windows machines.
# Do NOT force Xet high-performance by default: it can spike CPU/disk/RAM on very large depots.
# Users can still set HF_XET_HIGH_PERFORMANCE=1 manually before starting the tool.
if ([string]::IsNullOrWhiteSpace($env:HF_HUB_DISABLE_PROGRESS_BARS)) { $env:HF_HUB_DISABLE_PROGRESS_BARS = "0" }
if ([string]::IsNullOrWhiteSpace($env:HF_XET_HIGH_PERFORMANCE)) { $env:HF_XET_HIGH_PERFORMANCE = "0" }
if ([string]::IsNullOrWhiteSpace($env:PYTHONUTF8)) { $env:PYTHONUTF8 = "1" }

if ((-not $UseBuilderUpload) -and (-not (Test-Path -LiteralPath $incrementalUploadTool))) {
  throw "Incremental HF upload tool does not exist: $incrementalUploadTool"
}

if (-not (Test-Path -LiteralPath $CargoManifest)) {
  throw "Cargo manifest does not exist: $CargoManifest"
}

if ((-not $NoSyncMetadata) -and (-not (Test-Path -LiteralPath $SyncToolPath))) {
  throw "Metadata sync tool does not exist: $SyncToolPath"
}

$srcTauri = Split-Path -Parent $CargoManifest
$depotOut = Join-Path $DepotRoot $GameId
$builder = Join-Path $srcTauri "target\release\depot_builder.exe"

Write-Host "[SAFE] GameId        : $GameId"
Write-Host "[SAFE] Version       : $Version"
Write-Host "[SAFE] Input         : $InputDir"
Write-Host "[SAFE] Depot out     : $depotOut"
Write-Host "[SAFE] Repo          : $Repo"
Write-Host "[SAFE] Repo prefix   : $RepoPrefix"
Write-Host "[SAFE] Cargo         : $CargoManifest"
Write-Host "[SAFE] Encrypt packs : $(-not $NoEncryptPacks)"
Write-Host "[SAFE] Force rebuild : $ForceRebuild"
Write-Host "[SAFE] Sync metadata : $(-not $NoSyncMetadata)"
Write-Host "[SAFE] Extend existing: $(-not $NoExtendExisting)"
Write-Host "[SAFE] Delete source after pack: $DeleteSourceAfterPack"
Write-Host "[SAFE] Pack target   : $PackTargetMb MiB"
Write-Host "[SAFE] Pack prefix   : $PackIdPrefix"
Write-Host "[SAFE] Pack start    : $PackStartIndex"
$uploadModeName = if ($UseBuilderUpload) { "builder legacy" } else { "incremental per-file" }
Write-Host "[SAFE] HF upload mode : $uploadModeName"
Write-Host "[SAFE] HF progress bars: $env:HF_HUB_DISABLE_PROGRESS_BARS"
Write-Host "[SAFE] Xet high perf    : $env:HF_XET_HIGH_PERFORMANCE"

New-Item -ItemType Directory -Force -Path $depotOut | Out-Null

if (-not $NoSyncMetadata) {
  Write-Host "[SAFE] Sync remote metadata only, not packs..."
  python $SyncToolPath `
    --repo $Repo `
    --repo-type $RepoType `
    --prefix $RepoPrefix `
    --out $depotOut
} else {
  Write-Host "[SAFE] Skipping remote metadata sync because -NoSyncMetadata was set."
}

if (-not $UploadOnly) {
  if ($ForceRebuild -and (Test-Path -LiteralPath $builder)) {
    Write-Host "[SAFE] Force rebuild requested. Removing old builder: $builder"
    Remove-Item -LiteralPath $builder -Force
  }

  if (-not (Test-Path -LiteralPath $builder)) {
    Write-Host "[SAFE] Release depot_builder not found. Building release..."
    Push-Location $srcTauri
    try {
      cargo build --release --manifest-path $CargoManifest --bin depot_builder
    } finally {
      Pop-Location
    }
  } else {
    Write-Host "[SAFE] Using release builder: $builder"
  }

  Push-Location $srcTauri
  try {
    $builderArgs = @(
      "build-version",
      "--input", $InputDir,
      "--version", $Version,
      "--out", $depotOut,
      "--game-id", $GameId,
      "--pack-target-mb", ([string]$PackTargetMb),
      "--pack-id-prefix", $PackIdPrefix,
      "--pack-start-index", ([string]$PackStartIndex)
    )

    if ($UseBuilderUpload -or $UploadPacksIncrementally) {
      $builderArgs += @("--upload-repo", $Repo, "--repo-type", $RepoType, "--repo-prefix", $RepoPrefix)
    } else {
      Write-Host "[SAFE] Builder upload disabled. Build local depot first, then upload incrementally per file."
    }

    if (-not $NoExtendExisting) {
      $builderArgs += "--extend-existing"
    }

    if (-not [string]::IsNullOrWhiteSpace($LaunchOptionsJson)) {
      if ($LaunchOptionsJson -eq "ENV") {
        $json = $env:LAUNCH_OPTIONS_JSON
        if (-not [string]::IsNullOrWhiteSpace($json)) {
          $builderArgs += @("--launch-options-json", $json)
        }
      } else {
        $json = $LaunchOptionsJson
        $builderArgs += @("--launch-options-json", $json)
      }
    } elseif (-not [string]::IsNullOrWhiteSpace($LaunchExecutable)) {
      $builderArgs += @("--launch-executable", $LaunchExecutable)
    }

    if ($KeepLocalPacks -or (-not $UseBuilderUpload)) {
      $builderArgs += "--keep-local-packs"
    }

    if ($NoEncryptPacks) {
      $builderArgs += "--no-encrypt-packs"
    } else {
      # Explicit flag for readable logs. New builders accept it; old builders ignore it but still default to encryption only if they are actually new.
      $builderArgs += "--encrypt-packs"
    }

    if ($DeleteSourceAfterPack) {
      Write-Host "[SAFE] WARNING: --delete-source-after-pack is enabled. Source files will be deleted as they are packed!"
      $builderArgs += "--delete-source-after-pack"
    }

    if ($UploadPacksIncrementally) {
      Write-Host "[SAFE] INCREMENTAL MODE: Each pack will be uploaded and deleted immediately after creation to save disk space."
      $builderArgs += "--upload-packs-incrementally"
    }

    Write-Host "[SAFE] Running depot_builder build-version..."
    if ($NoExtendExisting) { Write-Host "[SAFE] --extend-existing is disabled for this run." }
    & $builder @builderArgs
    if ($LASTEXITCODE -ne 0) {
      throw "depot_builder failed with exit code $LASTEXITCODE"
    }
  } finally {
    Pop-Location
  }

  # Hard local safety check when encrypted packs are expected. This catches stale/old depot_builder.exe immediately.
  if (-not $NoEncryptPacks) {
    $packDir = Join-Path $depotOut "packs"
    if (Test-Path -LiteralPath $packDir) {
      $plainPacks = @()
      Get-ChildItem -LiteralPath $packDir -Filter "*.bin" | ForEach-Object {
        if (Test-ZstdMagicAtStart $_.FullName) { $plainPacks += $_.Name }
      }
      if ($plainPacks.Count -gt 0) {
        throw "Encryption was requested, but these packs still start with ZSTD magic 28 B5 2F FD: $($plainPacks -join ', '). This means an old/plain builder path is still being used."
      }
      Write-Host "[SAFE] Local encrypted-pack sanity check passed. No pack starts with ZSTD magic."
    } else {
      Write-Host "[SAFE] Pack folder not present after upload; skip local magic check. Enable KeepLocalPacks for manual verification."
    }
  }
} else {
  Write-Host "[SAFE] UploadOnly flag is set. Skipping depot builder and proceeding directly to incremental HF uploader."
}

if ($UploadOnly -and $UploadPacksIncrementally) {
  throw "ERROR: You cannot use both 'Upload Only' and 'Upload & delete incrementally'. Incremental mode deletes packs during build, so there are no local packs left to 'just upload'. Please uncheck one of them."
}

if ((-not $UseBuilderUpload) -and (-not $UploadPacksIncrementally)) {
  Write-Host "[SAFE] Uploading depot with incremental per-file HF uploader; no .hf_upload_stage will be created."
  $uploadArgs = @(
    $incrementalUploadTool,
    "--local-depot", $depotOut,
    "--repo", $Repo,
    "--repo-type", $RepoType,
    "--prefix", $RepoPrefix
  )
  if (-not $KeepLocalPacks) {
    $uploadArgs += "--delete-local-packs"
  }
  python @uploadArgs
  if ($LASTEXITCODE -ne 0) {
    throw "incremental HF upload failed with exit code $LASTEXITCODE"
  }
}


$verifyScript = @'
from huggingface_hub import hf_hub_download
import json
import os
import sys
repo = sys.argv[1]
repo_type = sys.argv[2]
prefix = sys.argv[3]
expected = sys.argv[4]
path = hf_hub_download(repo, repo_type=repo_type, filename=f"{prefix}/catalog.json", force_download=True, token=os.environ.get("HF_TOKEN"))
with open(path, "r", encoding="utf-8") as f:
    catalog = json.load(f)
latest = catalog.get("latestVersion")
versions = [v.get("version") for v in catalog.get("versions", [])]
if latest != expected or expected not in versions:
    raise SystemExit(f"remote catalog verification failed: latest={latest}, versions={versions}")
print(f"remote catalog verified: latest={latest}, versions={versions}")
'@
Write-Host "[SAFE] Verifying remote catalog..."
$verifyScript | python - $Repo $RepoType $RepoPrefix $Version
