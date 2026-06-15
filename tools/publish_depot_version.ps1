param(
  [Parameter(Mandatory = $true)]
  [string]$GameId,

  [Parameter(Mandatory = $true)]
  [string]$Version,

  [Parameter(Mandatory = $true)]
  [string]$InputDir,

  [string]$LaunchExecutable = "",
  [string]$Repo = "CatManga/Cat-Manga",
  [string]$RepoType = "dataset",
  [string]$RepoPrefix = "",
  [string]$DepotRoot = "E:\007Launcher\depot",
  [switch]$KeepLocalPacks
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($RepoPrefix)) {
  $RepoPrefix = $GameId
}

$scriptRoot = Split-Path -Parent $PSCommandPath
$launcherRoot = Split-Path -Parent $scriptRoot
$srcTauri = Join-Path $launcherRoot "src-tauri"
$depotOut = Join-Path $DepotRoot $GameId
$builder = Join-Path $srcTauri "target\release\depot_builder.exe"
$syncTool = Join-Path $scriptRoot "sync_hf_depot_metadata.py"

if (-not (Test-Path -LiteralPath $InputDir)) {
  throw "Input folder does not exist: $InputDir"
}

if (-not [string]::IsNullOrWhiteSpace($LaunchExecutable)) {
  $exePath = Join-Path $InputDir $LaunchExecutable
  if (-not (Test-Path -LiteralPath $exePath)) {
    throw "Launch executable was not found: $exePath"
  }
}

if ([string]::IsNullOrWhiteSpace($env:HF_TOKEN)) {
  throw "HF_TOKEN is not set in this PowerShell session."
}

if (-not (Test-Path -LiteralPath $builder)) {
  Push-Location $srcTauri
  try {
    cargo build --release --bin depot_builder
  } finally {
    Pop-Location
  }
}

New-Item -ItemType Directory -Force -Path $depotOut | Out-Null

python $syncTool `
  --repo $Repo `
  --repo-type $RepoType `
  --prefix $RepoPrefix `
  --out $depotOut

Push-Location $srcTauri
try {
  $builderArgs = @(
    "build-version",
    "--input", $InputDir,
    "--version", $Version,
    "--out", $depotOut,
    "--game-id", $GameId,
    "--extend-existing",
    "--upload-repo", $Repo,
    "--repo-type", $RepoType,
    "--repo-prefix", $RepoPrefix
  )

  if (-not [string]::IsNullOrWhiteSpace($LaunchExecutable)) {
    $builderArgs += @("--launch-executable", $LaunchExecutable)
  }

  if ($KeepLocalPacks) {
    $builderArgs += "--keep-local-packs"
  }

  & $builder @builderArgs
  if ($LASTEXITCODE -ne 0) {
    throw "depot_builder failed with exit code $LASTEXITCODE"
  }
} finally {
  Pop-Location
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
$verifyScript | python - $Repo $RepoType $RepoPrefix $Version
