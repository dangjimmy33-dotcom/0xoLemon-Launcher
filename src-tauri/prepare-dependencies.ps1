param (
    [switch]$VerifyOnly
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$manifestPath = Join-Path $PSScriptRoot "dependency-bundle.json"
$resourceRoot = Join-Path $PSScriptRoot "resources\redist"
$cacheRoot = Join-Path (Split-Path -Parent $PSScriptRoot) ".dependency-downloads\redist"

function Get-NormalizedHash {
    param ([string]$Path)

    $stream = [System.IO.File]::OpenRead($Path)
    $algorithm = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = $algorithm.ComputeHash($stream)
        ([System.BitConverter]::ToString($bytes)).Replace('-', '')
    } finally {
        $algorithm.Dispose()
        $stream.Dispose()
    }
}

function Assert-ExpectedFile {
    param (
        [string]$Path,
        [string]$ExpectedHash,
        [long]$MinimumBytes,
        [string]$Label
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "$Label is missing at $Path"
    }
    $file = Get-Item -LiteralPath $Path
    if ($file.Length -lt $MinimumBytes) {
        throw "$Label is unexpectedly small: $($file.Length) bytes (minimum $MinimumBytes)."
    }
    $actualHash = Get-NormalizedHash -Path $Path
    if ($actualHash -ne $ExpectedHash.ToUpperInvariant()) {
        throw "$Label SHA-256 mismatch. Expected $ExpectedHash, received $actualHash."
    }
}

function Assert-PublisherSignature {
    param (
        [string]$Path,
        [string]$Label,
        [string]$Publisher
    )

    if (-not (Get-Command Get-AuthenticodeSignature -ErrorAction SilentlyContinue)) {
        $securityModule = Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1'
        if (Test-Path -LiteralPath $securityModule -PathType Leaf) {
            Import-Module -Name $securityModule -ErrorAction Stop
        }
    }
    if (-not (Get-Command Get-AuthenticodeSignature -ErrorAction SilentlyContinue)) {
        throw "PowerShell Authenticode verification is unavailable for $Label."
    }
    $signature = Microsoft.PowerShell.Security\Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
        throw "$Label does not have a valid Authenticode signature: $($signature.Status)."
    }
    if (-not $signature.SignerCertificate -or $signature.SignerCertificate.Subject -notmatch [regex]::Escape($Publisher)) {
        throw "$Label is not signed by $Publisher."
    }
}

function Assert-EmbeddedArchiveInstaller {
    param (
        $Package,
        [string]$ArchivePath
    )

    if ([string]$Package.archiveKind -ne 'zip') {
        return
    }
    if ([string]::IsNullOrWhiteSpace([string]$Package.extractedInstallerPath) -or
        [string]::IsNullOrWhiteSpace([string]$Package.extractedInstallerSha256) -or
        [string]::IsNullOrWhiteSpace([string]$Package.extractedSignaturePublisher)) {
        throw "Archive metadata is incomplete for $($Package.id)."
    }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($ArchivePath)
    try {
        $entryName = ([string]$Package.extractedInstallerPath).Replace('\', '/')
        $entries = @($archive.Entries | Where-Object { $_.FullName -ceq $entryName })
        if ($entries.Count -ne 1 -or $entries[0].Name.Length -eq 0) {
            throw "$($Package.id) archive does not contain exactly one safe $entryName entry."
        }
        $verificationRoot = Join-Path $cacheRoot 'embedded-verification'
        [void](New-Item -ItemType Directory -Force -Path $verificationRoot)
        $verificationPath = Join-Path $verificationRoot ("$($Package.id)-" + [System.IO.Path]::GetFileName($entryName))
        $input = $entries[0].Open()
        $output = [System.IO.File]::Open($verificationPath, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
        try {
            $input.CopyTo($output)
            $output.Flush($true)
        } finally {
            $output.Dispose()
            $input.Dispose()
        }
        Assert-ExpectedFile -Path $verificationPath -ExpectedHash $Package.extractedInstallerSha256 -MinimumBytes $Package.minimumExtractedInstallerBytes -Label "$($Package.id) embedded installer"
        Assert-PublisherSignature -Path $verificationPath -Label "$($Package.id) embedded installer" -Publisher $Package.extractedSignaturePublisher
    } finally {
        $archive.Dispose()
    }
}

function Get-VerifiedDownload {
    param ($Package)

    $downloadPath = Join-Path $cacheRoot ([string]$Package.downloadFileName)
    if (Test-Path -LiteralPath $downloadPath -PathType Leaf) {
        try {
            Assert-ExpectedFile -Path $downloadPath -ExpectedHash $Package.downloadSha256 -MinimumBytes $Package.minimumDownloadBytes -Label $Package.id
            if (-not [string]::IsNullOrWhiteSpace([string]$Package.signaturePublisher)) {
                Assert-PublisherSignature -Path $downloadPath -Label $Package.id -Publisher $Package.signaturePublisher
            }
            Assert-EmbeddedArchiveInstaller -Package $Package -ArchivePath $downloadPath
            return $downloadPath
        } catch {
            if ($VerifyOnly) { throw }
        }
    }

    if ($VerifyOnly) {
        throw "Verified download cache is missing for $($Package.id): $downloadPath"
    }

    [void](New-Item -ItemType Directory -Force -Path $cacheRoot)
    $temporaryPath = "$downloadPath.download"
    Invoke-WebRequest -UseBasicParsing -Uri $Package.url -OutFile $temporaryPath
    Assert-ExpectedFile -Path $temporaryPath -ExpectedHash $Package.downloadSha256 -MinimumBytes $Package.minimumDownloadBytes -Label $Package.id
    if (-not [string]::IsNullOrWhiteSpace([string]$Package.signaturePublisher)) {
        Assert-PublisherSignature -Path $temporaryPath -Label $Package.id -Publisher $Package.signaturePublisher
    }
    Assert-EmbeddedArchiveInstaller -Package $Package -ArchivePath $temporaryPath
    Move-Item -LiteralPath $temporaryPath -Destination $downloadPath -Force
    $downloadPath
}

function Publish-BundledPayload {
    param (
        $Package,
        [string]$DownloadPath
    )

    $bundlePath = Join-Path $resourceRoot ([string]$Package.bundledPath).Replace('/', '\')
    [void](New-Item -ItemType Directory -Force -Path (Split-Path -Parent $bundlePath))
    $needsCopy = $true
    if (Test-Path -LiteralPath $bundlePath -PathType Leaf) {
        try {
            Assert-ExpectedFile -Path $bundlePath -ExpectedHash $Package.payloadSha256 -MinimumBytes $Package.minimumPayloadBytes -Label $Package.id
            $needsCopy = $false
        } catch {
            if ($VerifyOnly) { throw }
        }
    }
    if ($needsCopy) {
        if ($VerifyOnly) {
            throw "Bundled payload is missing or invalid for $($Package.id)."
        }
        Copy-Item -LiteralPath $DownloadPath -Destination $bundlePath -Force
    }

    Assert-ExpectedFile -Path $bundlePath -ExpectedHash $Package.payloadSha256 -MinimumBytes $Package.minimumPayloadBytes -Label $Package.id
    if (-not [string]::IsNullOrWhiteSpace([string]$Package.signaturePublisher)) {
        Assert-PublisherSignature -Path $bundlePath -Label $Package.id -Publisher $Package.signaturePublisher
    }
    Assert-EmbeddedArchiveInstaller -Package $Package -ArchivePath $bundlePath
    Get-Item -LiteralPath $bundlePath
}

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Dependency bundle manifest is missing: $manifestPath"
}

$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
if ($manifest.schema -ne 1 -or
    -not $manifest.packages -or $manifest.packages.Count -eq 0 -or
    -not $manifest.defaultDependencies -or $manifest.defaultDependencies.Count -eq 0 -or
    -not $manifest.gameProfiles) {
    throw "dependency-bundle.json is empty or uses an unsupported schema."
}

$knownDependencyIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
foreach ($package in $manifest.packages) {
    if (-not $knownDependencyIds.Add([string]$package.id)) {
        throw "Duplicate dependency ID: $($package.id)"
    }
    foreach ($alias in @($package.aliases)) {
        if (-not $knownDependencyIds.Add([string]$alias)) {
            throw "Duplicate dependency alias: $alias"
        }
    }
}
$profileReferences = @($manifest.defaultDependencies)
foreach ($profile in $manifest.gameProfiles.psobject.Properties) {
    if ([string]::IsNullOrWhiteSpace($profile.Name) -or @($profile.Value).Count -eq 0) {
        throw "Empty game dependency profile: $($profile.Name)"
    }
    $profileReferences += @($profile.Value)
}
foreach ($dependencyId in $profileReferences) {
    if ($dependencyId -ne 'ea-app' -and -not $knownDependencyIds.Contains([string]$dependencyId)) {
        throw "Dependency profile references an unknown package: $dependencyId"
    }
}

[void](New-Item -ItemType Directory -Force -Path $resourceRoot)
$published = @()
foreach ($package in $manifest.packages) {
    if ([string]::IsNullOrWhiteSpace($package.id) -or
        [string]::IsNullOrWhiteSpace($package.url) -or
        [string]::IsNullOrWhiteSpace($package.downloadSha256) -or
        [string]::IsNullOrWhiteSpace($package.payloadSha256)) {
        throw "Dependency bundle contains an incomplete package entry."
    }
    $download = Get-VerifiedDownload -Package $package
    $published += Publish-BundledPayload -Package $package -DownloadPath $download
}

$totalBytes = ($published | Measure-Object -Property Length -Sum).Sum
Write-Host "Dependency bundle ready: $($published.Count) verified offline packages, $([math]::Round($totalBytes / 1MB, 1)) MiB." -ForegroundColor Green
