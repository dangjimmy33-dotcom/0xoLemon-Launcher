param(
  [switch]$Force
)

$ErrorActionPreference = 'Stop'
$TauriRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $TauriRoot
$VendorRoot = Join-Path $ProjectRoot 'src\vendor\gse-uc-setup'
$Bridge = Join-Path $TauriRoot 'gse-core\gse_core_bridge.py'
$Requirements = Join-Path $TauriRoot 'gse-core-requirements.txt'
$ResourceBin = Join-Path $TauriRoot 'resources\gse-uc\bin'
$Output = Join-Path $ResourceBin 'gse-core.exe'
$BuildState = Join-Path $ResourceBin 'gse-core.build.json'
$Venv = Join-Path $TauriRoot '.gse-core-venv'
$Py = Join-Path $Venv 'Scripts\python.exe'

if (-not (Test-Path $VendorRoot)) { throw "Missing original GSE_UC_Setup source: $VendorRoot" }
if (-not (Test-Path $Bridge)) { throw "Missing headless GSE bridge: $Bridge" }

function Get-TreeHash {
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $files = @(
      Get-ChildItem (Join-Path $VendorRoot 'gse_autosetup') -Recurse -File -Filter '*.py'
      Get-Item $Bridge
      Get-Item $Requirements
    ) | Sort-Object FullName
    foreach ($file in $files) {
      $nameBytes = [Text.Encoding]::UTF8.GetBytes(($file.FullName.ToLowerInvariant()) + "`n")
      [void]$sha.TransformBlock($nameBytes, 0, $nameBytes.Length, $nameBytes, 0)
      $bytes = [IO.File]::ReadAllBytes($file.FullName)
      [void]$sha.TransformBlock($bytes, 0, $bytes.Length, $bytes, 0)
    }
    [void]$sha.TransformFinalBlock([byte[]]::new(0), 0, 0)
    return ([BitConverter]::ToString($sha.Hash)).Replace('-', '').ToLowerInvariant()
  }
  finally { $sha.Dispose() }
}

$sourceHash = Get-TreeHash
if (-not $Force -and (Test-Path $Output) -and (Test-Path $BuildState)) {
  try {
    $state = Get-Content $BuildState -Raw | ConvertFrom-Json
    if ($state.sourceHash -eq $sourceHash) {
      Write-Host "[GSE core] Exact original Python core sidecar is current: $Output"
      exit 0
    }
  } catch { }
}

function Find-RegularPython {
  $candidates = @(
    @{ Exe = 'py'; Args = @('-3.13') },
    @{ Exe = 'py'; Args = @('-3.12') },
    @{ Exe = 'py'; Args = @('-3.11') },
    @{ Exe = 'py'; Args = @('-3.10') },
    @{ Exe = 'py'; Args = @('-3.14') },
    @{ Exe = 'python'; Args = @() }
  )
  foreach ($candidate in $candidates) {
    try {
      & $candidate.Exe @($candidate.Args) -c "import sys,struct,sysconfig; ok=(struct.calcsize('P')*8==64 and (3,10)<=sys.version_info[:2]<(3,15) and sysconfig.get_config_var('Py_GIL_DISABLED')!=1); raise SystemExit(0 if ok else 1)" 2>$null
      if ($LASTEXITCODE -eq 0) { return $candidate }
    } catch { }
  }
  return $null
}

$selected = Find-RegularPython
if ($null -eq $selected) {
  throw 'No compatible regular 64-bit CPython 3.10-3.14 found. Install CPython 3.13 x64 (non-free-threaded) and rebuild.'
}
$baseExe = $selected.Exe
$baseArgs = @($selected.Args)

if (-not (Test-Path $Py)) {
  Write-Host '[GSE core] Creating isolated Python build environment...'
  & $baseExe @baseArgs -m venv $Venv
  if ($LASTEXITCODE -ne 0) { throw 'Could not create GSE core Python venv.' }
}

Write-Host '[GSE core] Installing headless build dependencies...'
& $Py -m pip install --disable-pip-version-check --quiet --upgrade pip
if ($LASTEXITCODE -ne 0) { throw 'pip upgrade failed for GSE core.' }
& $Py -m pip install --disable-pip-version-check --quiet -r $Requirements
if ($LASTEXITCODE -ne 0) { throw 'GSE core dependency installation failed.' }

Write-Host '[GSE core] Running original-core parity contracts...'
$oldPythonPath = $env:PYTHONPATH
try {
  $env:PYTHONPATH = "$VendorRoot;$TauriRoot\gse-core"
  & $Py (Join-Path $TauriRoot 'gse-core\parity_contract_test.py')
  if ($LASTEXITCODE -ne 0) { throw 'GSE original-core parity contracts failed.' }
}
finally { $env:PYTHONPATH = $oldPythonPath }

New-Item -ItemType Directory -Force $ResourceBin | Out-Null
$Work = Join-Path $TauriRoot '.gse-core-build'
$Spec = Join-Path $TauriRoot '.gse-core-spec'
Remove-Item $Work -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item $Spec -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $Work,$Spec | Out-Null

Write-Host '[GSE core] Building exact original GSE_UC_Setup headless sidecar...'
& $Py -m PyInstaller `
  --noconfirm --clean --onefile --console `
  --name gse-core `
  --distpath $ResourceBin `
  --workpath $Work `
  --specpath $Spec `
  --paths $VendorRoot `
  --exclude-module PySide6 `
  --collect-submodules googleapiclient `
  --collect-submodules google_auth_oauthlib `
  --hidden-import google.auth.transport.requests `
  --hidden-import googleapiclient.discovery `
  --hidden-import googleapiclient.http `
  --hidden-import google_auth_oauthlib.flow `
  $Bridge
if ($LASTEXITCODE -ne 0) { throw 'PyInstaller failed while building exact GSE core sidecar.' }
if (-not (Test-Path $Output)) { throw "GSE core sidecar was not produced: $Output" }

@{
  sourceHash = $sourceHash
  builtAt = (Get-Date).ToUniversalTime().ToString('o')
  bridge = 'src-tauri/gse-core/gse_core_bridge.py'
  source = 'src/vendor/gse-uc-setup/gse_autosetup'
} | ConvertTo-Json | Set-Content -Encoding UTF8 $BuildState

Remove-Item $Work -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item $Spec -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "[GSE core] Built: $Output"
