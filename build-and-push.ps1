param (
    [string]$CommitMessage = "Auto build and release",
    [string]$SigningKeyPath = "C:\Users\conte\.tauri\0xolemon.key",
    [string]$SigningPasswordPath = "C:\Users\conte\.tauri\0xolemon-signing-password.dpapi",
    [switch]$BuildOnly,
    [string[]]$ChangelogChanges = @(),
    [switch]$NoPause
)

$ErrorActionPreference = "Stop"

function Assert-NativeCommandSucceeded {
    param ([string]$Action)

    if ($LASTEXITCODE -ne 0) {
        throw "$Action failed with exit code $LASTEXITCODE."
    }
}

function Assert-ReleasePreflight {
    $manifestPath = Join-Path $PSScriptRoot "src-tauri\Cargo.toml"
    $metadataJson = cargo metadata --manifest-path $manifestPath --no-deps --format-version 1
    Assert-NativeCommandSucceeded "Reading Cargo metadata"

    $metadata = $metadataJson | ConvertFrom-Json
    $manifestFullPath = [System.IO.Path]::GetFullPath($manifestPath)
    $launcherPackages = @(
        $metadata.packages |
            Where-Object {
                [System.IO.Path]::GetFullPath([string]$_.manifest_path) -eq $manifestFullPath
            }
    )
    if ($launcherPackages.Count -ne 1) {
        throw "Release preflight expected one Cargo package for $manifestPath but found $($launcherPackages.Count)."
    }

    $cargoVersion = [string]$launcherPackages[0].version
    $binaryTargets = @(
        $launcherPackages[0].targets |
            Where-Object { $_.kind -contains "bin" }
    )
    $binaryNames = @($binaryTargets | ForEach-Object { [string]$_.name })
    if ($binaryNames.Count -ne 1 -or $binaryNames[0] -ne "0xoLemon") {
        $actual = if ($binaryNames.Count -gt 0) { $binaryNames -join ", " } else { "none" }
        throw "Release preflight requires exactly one app binary named '0xoLemon'. Cargo declares: $actual. Move utility binaries to tools\launcher-tools."
    }

    $sourceBinPath = Join-Path $PSScriptRoot "src-tauri\src\bin"
    $unexpectedSourceBins = @()
    if (Test-Path -LiteralPath $sourceBinPath -PathType Container) {
        $unexpectedSourceBins = @(Get-ChildItem -LiteralPath $sourceBinPath -File -Recurse)
    }
    if ($unexpectedSourceBins.Count -gt 0) {
        $unexpected = $unexpectedSourceBins.Name -join ", "
        throw "Release preflight found utility source files under src-tauri\src\bin: $unexpected. Tauri CLI bundles every file in that directory; move them to tools\launcher-tools."
    }

    $packageJsonPath = Join-Path $PSScriptRoot "package.json"
    $tauriConfigPath = Join-Path $PSScriptRoot "src-tauri\tauri.conf.json"
    $packageVersion = [string](Get-Content -Raw -LiteralPath $packageJsonPath | ConvertFrom-Json).version
    $tauriVersion = [string](Get-Content -Raw -LiteralPath $tauriConfigPath | ConvertFrom-Json).version
    if ([string]::IsNullOrWhiteSpace($packageVersion) -or $packageVersion -ne $tauriVersion -or $packageVersion -ne $cargoVersion) {
        throw "Release preflight found mismatched versions: package.json=$packageVersion, tauri.conf.json=$tauriVersion, src-tauri/Cargo.toml=$cargoVersion."
    }

    Write-Host "Preflight OK: 0xoLemon is the only Tauri binary; version $packageVersion is synchronized." -ForegroundColor Green
}

function Assert-ReleaseArtifacts {
    param ([datetime]$BuildStartedAtUtc)

    $bundleRoot = Join-Path $PSScriptRoot "src-tauri\target\release\bundle"
    if (-not (Test-Path -LiteralPath $bundleRoot -PathType Container)) {
        throw "Tauri build completed without a bundle directory at $bundleRoot"
    }

    $freshFiles = @(
        Get-ChildItem -LiteralPath $bundleRoot -File -Recurse |
            Where-Object { $_.LastWriteTimeUtc -ge $BuildStartedAtUtc -and $_.Length -gt 0 }
    )
    $msiFiles = @($freshFiles | Where-Object { $_.Extension -ieq ".msi" })
    $nsisFiles = @(
        $freshFiles |
            Where-Object { $_.Extension -ieq ".exe" -and $_.FullName -match "[\\/]nsis[\\/]" }
    )
    $signatureFiles = @($freshFiles | Where-Object { $_.Extension -ieq ".sig" })

    if ($msiFiles.Count -lt 1) { throw "Build verification did not find a fresh MSI bundle." }
    if ($nsisFiles.Count -lt 1) { throw "Build verification did not find a fresh NSIS installer." }
    if ($signatureFiles.Count -lt 1) { throw "Build verification did not find an updater signature." }

    $updaterArtifacts = @()
    foreach ($signature in $signatureFiles) {
        $signedPath = $signature.FullName.Substring(0, $signature.FullName.Length - ".sig".Length)
        if (-not (Test-Path -LiteralPath $signedPath -PathType Leaf)) {
            throw "Updater signature has no matching artifact: $($signature.FullName)"
        }
        $signedArtifact = Get-Item -LiteralPath $signedPath
        if ($signedArtifact.Length -le 0 -or $signedArtifact.LastWriteTimeUtc -lt $BuildStartedAtUtc) {
            throw "Updater signature points to a stale or empty artifact: $signedPath"
        }
        $updaterArtifacts += $signedArtifact
    }
    if ($updaterArtifacts.Count -lt 1) { throw "Build verification did not find a signed updater artifact." }

    Write-Host "Verified $($msiFiles.Count) MSI, $($nsisFiles.Count) NSIS, $($updaterArtifacts.Count) signed updater artifact(s), and $($signatureFiles.Count) signature(s)." -ForegroundColor Green
}

function Invoke-VerifiedTauriBuild {
    $buildStartedAtUtc = [DateTime]::UtcNow.AddSeconds(-2)
    Write-Host "Running npm run tauri build. This can take several minutes." -ForegroundColor Yellow
    npm run tauri build
    Assert-NativeCommandSucceeded "Tauri build"
    Assert-ReleaseArtifacts -BuildStartedAtUtc $buildStartedAtUtc
}

function Save-ReleaseMetadata {
    param ([string[]]$Paths)

    $snapshots = @{}
    foreach ($path in $Paths) {
        $absolutePath = Join-Path $PSScriptRoot $path
        if (Test-Path -LiteralPath $absolutePath -PathType Leaf) {
            $snapshots[$path] = [System.IO.File]::ReadAllBytes($absolutePath)
        }
    }
    return $snapshots
}

function Restore-ReleaseMetadata {
    param ([hashtable]$Snapshots)

    foreach ($entry in $Snapshots.GetEnumerator()) {
        $absolutePath = Join-Path $PSScriptRoot $entry.Key
        [System.IO.File]::WriteAllBytes($absolutePath, [byte[]]$entry.Value)
    }
}

function Get-GitBuildOutputPaths {
    param ([string[]]$Paths)

    @(
        $Paths |
            Where-Object {
                $_ -match '(^|/)target/' -or
                $_ -match '^\.playwright-mcp/' -or
                $_ -match '^playwright-report/' -or
                $_ -match '^test-results/' -or
                $_ -eq 'launcher-shell-bg-check.png'
            }
    )
}

function Assert-NoTrackedBuildOutputs {
    $trackedOutputs = Get-GitBuildOutputPaths -Paths @(git ls-files)
    Assert-NativeCommandSucceeded "Scanning tracked files"
    if ($trackedOutputs.Count -gt 0) {
        $preview = ($trackedOutputs | Select-Object -First 12) -join "`n  "
        throw "Refusing to release while build/test outputs are tracked by Git:`n  $preview`nRemove them from the Git index first; keep local build files ignored."
    }

    $stagedOutputs = Get-GitBuildOutputPaths -Paths @(git diff --cached --name-only)
    Assert-NativeCommandSucceeded "Scanning staged files"
    if ($stagedOutputs.Count -gt 0) {
        $preview = ($stagedOutputs | Select-Object -First 12) -join "`n  "
        throw "Refusing to commit build/test outputs:`n  $preview"
    }
}

function Invoke-ReleaseGitStage {
    Assert-NoTrackedBuildOutputs
    git add --all -- .
    Assert-NativeCommandSucceeded "Staging release changes"
    Assert-NoTrackedBuildOutputs
}

function Initialize-SigningEnvironment {
    Add-Type -AssemblyName System.Security

    if (-not (Test-Path -LiteralPath $SigningKeyPath -PathType Leaf)) {
        throw "Tauri signing key was not found at $SigningKeyPath"
    }

    $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw -LiteralPath $SigningKeyPath
    if ($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
        return
    }

    if (Test-Path -LiteralPath $SigningPasswordPath -PathType Leaf) {
        try {
            $protectedPassword = [System.IO.File]::ReadAllBytes($SigningPasswordPath)
            $passwordBytes = [System.Security.Cryptography.ProtectedData]::Unprotect(
                $protectedPassword,
                $null,
                [System.Security.Cryptography.DataProtectionScope]::CurrentUser
            )
            try {
                $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [System.Text.Encoding]::UTF8.GetString($passwordBytes)
            } finally {
                [System.Array]::Clear($passwordBytes, 0, $passwordBytes.Length)
            }
            return
        } catch {
            throw "Unable to decrypt the Tauri signing password at $SigningPasswordPath for the current Windows user."
        }
    }

    $securePassword = Read-Host "Tauri signing key password" -AsSecureString
    $passwordPointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($securePassword)
    try {
        $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($passwordPointer)
    } finally {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($passwordPointer)
    }

    $passwordDirectory = Split-Path -Parent $SigningPasswordPath
    if (-not (Test-Path -LiteralPath $passwordDirectory -PathType Container)) {
        [void][System.IO.Directory]::CreateDirectory($passwordDirectory)
    }
    $passwordBytes = [System.Text.Encoding]::UTF8.GetBytes($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)
    try {
        $protectedPassword = [System.Security.Cryptography.ProtectedData]::Protect(
            $passwordBytes,
            $null,
            [System.Security.Cryptography.DataProtectionScope]::CurrentUser
        )
        [System.IO.File]::WriteAllBytes($SigningPasswordPath, $protectedPassword)
    } finally {
        [System.Array]::Clear($passwordBytes, 0, $passwordBytes.Length)
    }
}

function Set-CargoPackageVersion {
    param ([string]$Version)

    if ($Version -notmatch '^\d+\.\d+\.\d+$') {
        throw "Refusing to write invalid Cargo package version: $Version"
    }
    $manifestPath = Join-Path $PSScriptRoot "src-tauri\Cargo.toml"
    $content = [System.IO.File]::ReadAllText($manifestPath)
    $packageMatch = [regex]::Match($content, '(?ms)^\[package\]\s*(?<body>.*?)(?=^\[)')
    if (-not $packageMatch.Success) {
        throw "Unable to locate the [package] section in $manifestPath"
    }
    $body = $packageMatch.Groups['body'].Value
    $versionMatches = [regex]::Matches($body, '(?m)^version\s*=\s*"[^"]+"\s*$')
    if ($versionMatches.Count -ne 1) {
        throw "Expected exactly one package version in $manifestPath but found $($versionMatches.Count)."
    }
    $updatedBody = [regex]::Replace(
        $body,
        '(?m)^version\s*=\s*"[^"]+"\s*$',
        "version = `"$Version`""
    )
    $updated = $content.Substring(0, $packageMatch.Groups['body'].Index) +
        $updatedBody +
        $content.Substring($packageMatch.Groups['body'].Index + $packageMatch.Groups['body'].Length)
    [System.IO.File]::WriteAllText($manifestPath, $updated, [System.Text.UTF8Encoding]::new($false))
}

Set-Location -LiteralPath $PSScriptRoot

Write-Host "=== STEP 1: PREPARE RELEASE ENVIRONMENT ===" -ForegroundColor Cyan
if (Test-Path Env:\GITHUB_TOKEN) {
    Remove-Item Env:\GITHUB_TOKEN
    Write-Host "Removed GITHUB_TOKEN from the current process." -ForegroundColor Yellow
}

Assert-ReleasePreflight
Initialize-SigningEnvironment

if ($BuildOnly) {
    Write-Host "`n=== STEP 2: BUILD AND VERIFY LOCAL ARTIFACTS ===" -ForegroundColor Cyan
    Invoke-VerifiedTauriBuild
    Write-Host "`n=== BUILD-ONLY COMPLETE ===" -ForegroundColor Green
    Write-Host "Versions, changelog, Git history, tags, and remotes were not changed." -ForegroundColor Yellow
    exit 0
}

$releaseMetadataPaths = @(
    "package.json",
    "package-lock.json",
    "src-tauri\Cargo.toml",
    "src-tauri\Cargo.lock",
    "src-tauri\tauri.conf.json",
    "src\changelog.json"
)
$releaseMetadataSnapshots = Save-ReleaseMetadata -Paths $releaseMetadataPaths

try {
    Write-Host "`n=== STEP 2: PREPARE NEXT VERSION ===" -ForegroundColor Cyan
    $newVersion = (npm version patch --no-git-tag-version | Select-Object -Last 1).Trim()
    Assert-NativeCommandSucceeded "Bumping package version"
    if ($newVersion -notmatch '^v\d+\.\d+\.\d+$') {
        throw "npm returned an invalid version: $newVersion"
    }
    Write-Host "Prepared version $newVersion" -ForegroundColor Yellow

    node -e "const fs=require('fs'); const p=require('./package.json'); const t=JSON.parse(fs.readFileSync('./src-tauri/tauri.conf.json','utf8')); t.version=p.version; fs.writeFileSync('./src-tauri/tauri.conf.json', JSON.stringify(t, null, 2) + '\n');"
    Assert-NativeCommandSucceeded "Synchronizing the Tauri version"
    Set-CargoPackageVersion -Version ($newVersion.TrimStart("v"))
    Assert-ReleasePreflight

    Write-Host "`n=== STEP 3: UPDATE CHANGELOG ===" -ForegroundColor Cyan
    $changelogFile = "src/changelog.json"
    $changes = @(
        $ChangelogChanges |
            Where-Object { $_ -match '\S' } |
            ForEach-Object { $_.Trim() -replace '^- ', '' -replace '^\* ', '' }
    )

    if ($changes.Count -eq 0) {
        Add-Type -AssemblyName System.Windows.Forms
        Add-Type -AssemblyName System.Drawing

        $form = New-Object System.Windows.Forms.Form
        $form.Text = "Changelog for $newVersion"
        $form.Size = New-Object System.Drawing.Size(600, 400)
        $form.StartPosition = "CenterScreen"
        $form.TopMost = $true

        $label = New-Object System.Windows.Forms.Label
        $label.Location = New-Object System.Drawing.Point(15, 15)
        $label.Size = New-Object System.Drawing.Size(550, 40)
        $label.Font = New-Object System.Drawing.Font("Arial", 10)
        $label.Text = "Enter one change per line. Leave empty to keep the existing changelog."

        $textBox = New-Object System.Windows.Forms.TextBox
        $textBox.Location = New-Object System.Drawing.Point(15, 60)
        $textBox.Size = New-Object System.Drawing.Size(550, 240)
        $textBox.Multiline = $true
        $textBox.ScrollBars = "Vertical"
        $textBox.Font = New-Object System.Drawing.Font("Consolas", 11)

        $okButton = New-Object System.Windows.Forms.Button
        $okButton.Location = New-Object System.Drawing.Point(465, 315)
        $okButton.Size = New-Object System.Drawing.Size(100, 30)
        $okButton.Text = "Save and continue"
        $okButton.DialogResult = [System.Windows.Forms.DialogResult]::OK

        $form.Controls.Add($label)
        $form.Controls.Add($textBox)
        $form.Controls.Add($okButton)
        $form.AcceptButton = $okButton

        $result = $form.ShowDialog()
        if ($result -eq [System.Windows.Forms.DialogResult]::OK -and -not [string]::IsNullOrWhiteSpace($textBox.Text)) {
            $changes = @(
                $textBox.Text -split "`n" |
                    Where-Object { $_ -match '\S' } |
                    ForEach-Object { $_.Trim() -replace '^- ', '' -replace '^\* ', '' }
            )
        }
    }

    if ($changes.Count -gt 0) {
        $jsonContent = Get-Content -Raw -Encoding UTF8 -LiteralPath $changelogFile | ConvertFrom-Json
        $newEntry = [ordered]@{
            version = $newVersion.TrimStart("v")
            date = (Get-Date -Format "yyyy-MM-dd")
            changes = @($changes)
        }
        @($newEntry) + $jsonContent |
            ConvertTo-Json -Depth 10 |
            Set-Content -LiteralPath $changelogFile -Encoding UTF8
        Write-Host "Updated changelog with $($changes.Count) entries." -ForegroundColor Green
    } else {
        Write-Host "Changelog unchanged." -ForegroundColor Yellow
    }

    Write-Host "`n=== STEP 4: BUILD AND SIGN TAURI APP ===" -ForegroundColor Cyan
    Invoke-VerifiedTauriBuild
    Write-Host "Build successful." -ForegroundColor Green
} catch {
    Restore-ReleaseMetadata -Snapshots $releaseMetadataSnapshots
    Write-Host "Release metadata was restored because the build did not complete." -ForegroundColor Yellow
    throw
}

Write-Host "`n=== STEP 5: COMMIT AND TAG VERIFIED BUILD ===" -ForegroundColor Cyan
Invoke-ReleaseGitStage

$hasChanges = git status --porcelain
if (-not $hasChanges) {
    throw "The verified build produced no changes to commit."
}

git commit -m "$CommitMessage ($newVersion)"
Assert-NativeCommandSucceeded "Committing the verified release"

git tag $newVersion
Assert-NativeCommandSucceeded "Creating release tag $newVersion"
Write-Host "Committed and tagged $newVersion." -ForegroundColor Green

Write-Host "`n=== STEP 6: PUSH BRANCH AND RELEASE TAG ===" -ForegroundColor Cyan
git push
Assert-NativeCommandSucceeded "Pushing the current branch"

git push origin $newVersion
Assert-NativeCommandSucceeded "Pushing release tag $newVersion"

Write-Host "`n=== COMPLETE ===" -ForegroundColor Green
Write-Host "GitHub Actions will build and publish the signed release for $newVersion." -ForegroundColor Yellow
Write-Host "Required GitHub secrets: TAURI_SIGNING_PRIVATE_KEY, TAURI_SIGNING_PRIVATE_KEY_PASSWORD, HF_REPOS_JSON" -ForegroundColor Cyan
if (-not $NoPause) {
    pause
}
