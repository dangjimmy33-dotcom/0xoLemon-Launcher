param(
    [switch]$Strict
)

$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$embedded = Join-Path $root 'resources\embedded'
$tmpRoot = Join-Path $root 'resources\.fetch-v18'
New-Item -ItemType Directory -Force -Path $embedded | Out-Null
New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null

function Write-ComponentInfo([string]$Dir, [string]$Source, [string]$Tag, [string]$Sha256 = '') {
    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    $obj = [ordered]@{ source = $Source; tag = $Tag }
    if ($Sha256) { $obj.sha256 = $Sha256 }
    $obj | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Dir 'component.json')
}

function Invoke-RestWithRetry([string]$Uri, [int]$Attempts = 4) {
    $last = $null
    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        try {
            return Invoke-RestMethod -Headers @{ 'User-Agent'='GSE-UC-Setup-Build/1.8.3'; 'Accept'='application/vnd.github+json' } -Uri $Uri -TimeoutSec 45
        } catch {
            $last = $_
            if ($attempt -lt $Attempts) {
                $delay = [Math]::Pow(2, $attempt)
                Write-Host ("      API retry {0}/{1} in {2}s..." -f $attempt, $Attempts, $delay)
                Start-Sleep -Seconds $delay
            }
        }
    }
    throw $last
}

function Invoke-DownloadWithRetry([string]$Uri, [string]$OutFile, [int]$Attempts = 4) {
    $part = "$OutFile.part"
    $lastMessage = 'download failed'
    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        Remove-Item -Force $part -ErrorAction SilentlyContinue
        try {
            Invoke-WebRequest -UseBasicParsing -Headers @{ 'User-Agent'='GSE-UC-Setup-Build/1.8.3' } -Uri $Uri -OutFile $part -TimeoutSec 180
            if (-not (Test-Path $part) -or (Get-Item $part).Length -le 0) { throw 'Downloaded file is empty.' }
            Move-Item -Force $part $OutFile
            return
        } catch {
            $lastMessage = $_.Exception.Message
            Remove-Item -Force $part -ErrorAction SilentlyContinue
            $curl = Get-Command curl.exe -ErrorAction SilentlyContinue
            if ($curl) {
                try {
                    & $curl.Source -L --fail --retry 3 --retry-delay 2 --connect-timeout 20 --output $part $Uri
                    if ($LASTEXITCODE -eq 0 -and (Test-Path $part) -and (Get-Item $part).Length -gt 0) {
                        Move-Item -Force $part $OutFile
                        return
                    }
                    $lastMessage = "curl.exe exited with code $LASTEXITCODE"
                } catch {
                    $lastMessage = $_.Exception.Message
                } finally {
                    Remove-Item -Force $part -ErrorAction SilentlyContinue
                }
            }
            if ($attempt -lt $Attempts) {
                $delay = [Math]::Pow(2, $attempt)
                Write-Host ("      Download retry {0}/{1} in {2}s..." -f $attempt, $Attempts, $delay)
                Start-Sleep -Seconds $delay
            }
        }
    }
    throw "Unable to download $Uri after $Attempts attempts: $lastMessage"
}

function Get-LatestRelease([string]$Repo) {
    return Invoke-RestWithRetry "https://api.github.com/repos/$Repo/releases/latest"
}

function Get-Asset($Release, [scriptblock]$Predicate) {
    foreach ($asset in $Release.assets) {
        if (& $Predicate $asset) { return $asset }
    }
    return $null
}

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -Path $Path).Hash.ToLowerInvariant()
}

function Verify-Asset([string]$Path, $Asset) {
    $digest = [string]$Asset.digest
    if ($digest -and $digest.ToLowerInvariant().StartsWith('sha256:')) {
        $expected = $digest.Substring(7).ToLowerInvariant()
        $actual = Get-Sha256 $Path
        if ($actual -ne $expected) { throw "SHA-256 mismatch for $($Asset.name): expected $expected, got $actual" }
        return $actual
    }
    return Get-Sha256 $Path
}

function Expand-ZipNormalized([string]$Archive, [string]$Stage) {
    if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
    New-Item -ItemType Directory -Force -Path $Stage | Out-Null
    Expand-Archive -LiteralPath $Archive -DestinationPath $Stage -Force
}

function Copy-Tree([string]$Source, [string]$Destination) {
    if (Test-Path $Destination) { Remove-Item -Recurse -Force $Destination }
    New-Item -ItemType Directory -Force -Path $Destination | Out-Null
    Copy-Item -Recurse -Force (Join-Path $Source '*') $Destination
}

function Assert-GseToolsBaseline {
    $dest = Join-Path $embedded 'gse_tools'
    if (-not (Test-Path $dest)) {
        throw 'Official gse_fork_tools baseline is missing. Re-run BUILD_EXE_V1_8_1.bat with network access.'
    }
    $generator = Get-ChildItem -Path $dest -Recurse -File | Where-Object { $_.Name -match '^generate_emu_config.*\.exe$' } | Select-Object -First 1
    if (-not $generator) {
        throw 'Official gse_fork_tools baseline is incomplete: generate_emu_config.exe was not found.'
    }
}

function Run-Optional([string]$Name, [scriptblock]$Action) {
    try {
        Write-Host "      Preparing $Name..."
        & $Action
        Write-Host "      ${Name}: OK"
    }
    catch {
        Write-Warning "$Name could not be refreshed: $($_.Exception.Message)"
        if ($Strict) { throw }
    }
}

# Keep the existing full GSE baseline and migrate_gse supplied with the project.
$gse = Join-Path $embedded 'gse'
if (-not (Test-Path (Join-Path $gse 'regular\x64\steam_api64.dll'))) {
    throw 'Full embedded GSE baseline is missing regular\x64\steam_api64.dll.'
}
if (-not (Test-Path (Join-Path $gse 'steamclient_experimental\GameOverlayRenderer64.dll'))) {
    throw 'Full embedded GSE ColdClient resources are incomplete.'
}
$migrate = Join-Path $embedded 'migrate_gse\migrate_gse.exe'
if (-not (Test-Path $migrate)) { throw 'Embedded migrate_gse.exe is missing.' }

Run-Optional 'Official gse_fork_tools baseline (alex47exe/gse_fork_tools)' {
    $release = Get-LatestRelease 'alex47exe/gse_fork_tools'
    $asset = Get-Asset $release { param($a) ([string]$a.name) -eq 'gen_emu_cfg-Windows-Release.7z' }
    if (-not $asset) { throw 'gen_emu_cfg-Windows-Release.7z was not found.' }
    $archive = Join-Path $tmpRoot 'gen_emu_cfg-Windows-Release.7z'
    Invoke-DownloadWithRetry $asset.browser_download_url $archive
    $sha = Verify-Asset $archive $asset
    $stage = Join-Path $tmpRoot 'gse_tools'
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory -Force -Path $stage | Out-Null
    $sevenZip = Join-Path $root 'resources\7zip\7za.exe'
    if (-not (Test-Path $sevenZip)) { throw 'resources\\7zip\\7za.exe is required to unpack official gse_fork_tools.' }
    & $sevenZip x -y "-o$stage" $archive | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "7za failed with exit code $LASTEXITCODE" }
    $generator = Get-ChildItem -Path $stage -Recurse -File | Where-Object { $_.Name -match '^generate_emu_config.*\.exe$' } | Select-Object -First 1
    if (-not $generator) { throw 'Official generator executable was not found after extraction.' }
    $dest = Join-Path $embedded 'gse_tools'
    Copy-Tree $stage $dest
    Write-ComponentInfo $dest 'alex47exe/gse_fork_tools' ([string]$release.tag_name) $sha
}
Assert-GseToolsBaseline

Run-Optional 'UC Online2 baseline (UnionCrax-Team/uc-online2)' {
    $release = Get-LatestRelease 'UnionCrax-Team/uc-online2'
    $asset = Get-Asset $release { param($a) $n=[string]$a.name; $n.EndsWith('.zip') -and $n.ToLowerInvariant().Contains('release') -and -not $n.ToLowerInvariant().Contains('debug') }
    if (-not $asset) { throw 'No release ZIP asset was found.' }
    $archive = Join-Path $tmpRoot 'uc-online2.zip'
    Invoke-DownloadWithRetry $asset.browser_download_url $archive
    $sha = Verify-Asset $archive $asset
    $stage = Join-Path $tmpRoot 'uc-online2'
    Expand-ZipNormalized $archive $stage
    $probe = Get-ChildItem -Path $stage -Recurse -File -Filter 'steam_api64.dll' | Select-Object -First 1
    if (-not $probe) { throw 'Release ZIP contains no steam_api64.dll.' }
    $dest = Join-Path $embedded 'uc_online'
    Copy-Tree $stage $dest
    Write-ComponentInfo $dest 'UnionCrax-Team/uc-online2' ([string]$release.tag_name) $sha
}

Run-Optional 'RUNE SteamStub baseline (Mush-iii/rune-emu)' {
    $release = Get-LatestRelease 'Mush-iii/rune-emu'
    $asset = Get-Asset $release { param($a) ([string]$a.name).ToLowerInvariant() -eq 'steamstub.zip' }
    if (-not $asset) { throw 'steamstub.zip was not found.' }
    $archive = Join-Path $tmpRoot 'steamstub.zip'
    Invoke-DownloadWithRetry $asset.browser_download_url $archive
    $sha = Verify-Asset $archive $asset
    $stage = Join-Path $tmpRoot 'steamstub'
    Expand-ZipNormalized $archive $stage
    $x64 = Get-ChildItem -Path $stage -Recurse -File -Filter 'steamstub_x64.dll' | Select-Object -First 1
    $x32 = Get-ChildItem -Path $stage -Recurse -File -Filter 'steamstub_x32.dll' | Select-Object -First 1
    if (-not $x64 -or -not $x32) { throw 'SteamStub ZIP is missing x86/x64 DLLs.' }
    $dest = Join-Path $embedded 'rune_steamstub'
    if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    Copy-Item -Force $x64.FullName (Join-Path $dest 'steamstub_x64.dll')
    Copy-Item -Force $x32.FullName (Join-Path $dest 'steamstub_x32.dll')
    Write-ComponentInfo $dest 'Mush-iii/rune-emu' ([string]$release.tag_name) $sha
}

Run-Optional 'Steamless baseline (atom0s/Steamless)' {
    $release = Get-LatestRelease 'atom0s/Steamless'
    $asset = Get-Asset $release { param($a) ([string]$a.name).ToLowerInvariant().EndsWith('.zip') }
    if (-not $asset) { throw 'Steamless release ZIP was not found.' }
    $archive = Join-Path $tmpRoot 'steamless.zip'
    Invoke-DownloadWithRetry $asset.browser_download_url $archive
    $sha = Verify-Asset $archive $asset
    $stage = Join-Path $tmpRoot 'steamless'
    Expand-ZipNormalized $archive $stage
    $cli = Get-ChildItem -Path $stage -Recurse -File -Filter 'Steamless.CLI.exe' | Select-Object -First 1
    if (-not $cli) { throw 'Steamless.CLI.exe was not found in release ZIP.' }
    $sourceRoot = $cli.Directory.FullName
    $dest = Join-Path $embedded 'steamless'
    Copy-Tree $sourceRoot $dest
    Write-ComponentInfo $dest 'atom0s/Steamless' ([string]$release.tag_name) $sha
}

$ucProbe = Get-ChildItem -Path (Join-Path $embedded 'uc_online') -Recurse -File -Filter 'steam_api64.dll' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $ucProbe) { throw 'UC Online2 embedded baseline is missing steam_api64.dll.' }
if (-not (Test-Path (Join-Path $embedded 'rune_steamstub\steamstub_x64.dll'))) { throw 'RUNE SteamStub x64 embedded baseline is missing.' }
if (-not (Test-Path (Join-Path $embedded 'steamless\Steamless.CLI.exe'))) { throw 'Steamless embedded baseline is missing Steamless.CLI.exe.' }

Remove-Item -Recurse -Force $tmpRoot -ErrorAction SilentlyContinue
Write-Host '      V1.8.3 hybrid embedded resources prepared.'
