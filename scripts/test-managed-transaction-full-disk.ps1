param(
    [ValidateRange(96, 512)]
    [int]$SizeMiB = 192
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'This OS fault harness requires an elevated PowerShell session to attach a disposable VHDX.'
}
if (-not (Get-Command diskpart.exe -ErrorAction SilentlyContinue)) {
    throw 'diskpart.exe is required for the disposable full-disk harness.'
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$faultRoot = Join-Path $repoRoot 'downloading\fault-harness'
[IO.Directory]::CreateDirectory($faultRoot) | Out-Null
$vhdxPath = Join-Path $faultRoot 'managed-transaction-full-disk.vhdx'
if (Test-Path -LiteralPath $vhdxPath) {
    throw "Refusing to overwrite an existing VHDX: $vhdxPath"
}

$usedLetters = @(Get-Volume -ErrorAction SilentlyContinue | Where-Object DriveLetter | ForEach-Object { [string]$_.DriveLetter })
$driveLetter = $null
for ($code = [int][char]'Z'; $code -ge [int][char]'R'; $code--) {
    $candidate = [string][char]$code
    if ($usedLetters -notcontains $candidate -and -not (Test-Path -LiteralPath "${candidate}:\")) {
        $driveLetter = $candidate
        break
    }
}
if (-not $driveLetter) {
    throw 'No unused drive letter is available in the safe R: through Z: range.'
}

$attached = $false
$startedAt = [DateTimeOffset]::UtcNow
$modesPassed = [Collections.Generic.List[string]]::new()
try {
    $createCommands = @(
        "create vdisk file=`"$vhdxPath`" maximum=$SizeMiB type=fixed",
        "select vdisk file=`"$vhdxPath`"",
        'attach vdisk',
        'create partition primary',
        'format fs=ntfs label=OXOFAULT quick',
        "assign letter=$driveLetter",
        'exit'
    )
    $diskpartOutput = $createCommands | & diskpart.exe 2>&1
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath "${driveLetter}:\")) {
        throw "Could not create and mount the disposable NTFS VHDX.`n$($diskpartOutput -join [Environment]::NewLine)"
    }
    $attached = $true

    $volumeRoot = "${driveLetter}:\"
    $markerPath = Join-Path $volumeRoot '.0xo-disposable-full-disk-test'
    [IO.File]::WriteAllText($markerPath, '0xo-disposable-full-disk-test-v1', [Text.UTF8Encoding]::new($false))

    Push-Location (Join-Path $repoRoot 'src-tauri')
    try {
        foreach ($mode in @('preflight', 'postPreflight')) {
            $env:OXO_TEST_FULL_DISK_ROOT = $volumeRoot
            $env:OXO_TEST_FULL_DISK_MODE = $mode
            & cargo test --lib 'managed_file_transaction::tests::full_disk_transaction_helper' -- --ignored --exact --test-threads=1
            if ($LASTEXITCODE -ne 0) {
                throw "Managed transaction full-disk mode failed: $mode"
            }
            $modesPassed.Add($mode)
        }
    }
    finally {
        Remove-Item Env:OXO_TEST_FULL_DISK_ROOT -ErrorAction SilentlyContinue
        Remove-Item Env:OXO_TEST_FULL_DISK_MODE -ErrorAction SilentlyContinue
        Remove-Item Env:OXO_TEST_FILL_DISK_AFTER_PREFLIGHT -ErrorAction SilentlyContinue
        Pop-Location
    }

    $evidence = [ordered]@{
        schemaVersion = 1
        test = 'managed-file-transaction-full-disk'
        filesystem = 'NTFS'
        fixedVhdxMiB = $SizeMiB
        modesPassed = @($modesPassed)
        startedAt = $startedAt.ToString('O')
        completedAt = [DateTimeOffset]::UtcNow.ToString('O')
        powerLossTested = $false
    } | ConvertTo-Json -Depth 4
    [IO.File]::WriteAllText(
        (Join-Path $faultRoot 'last-full-disk-run.json'),
        $evidence,
        [Text.UTF8Encoding]::new($false)
    )
}
finally {
    if ($attached -or (Test-Path -LiteralPath $vhdxPath)) {
        $detachCommands = @(
            "select vdisk file=`"$vhdxPath`"",
            'detach vdisk',
            'exit'
        )
        $null = $detachCommands | & diskpart.exe 2>&1
    }
    if (Test-Path -LiteralPath $vhdxPath) {
        Remove-Item -LiteralPath $vhdxPath -Force
    }
}
