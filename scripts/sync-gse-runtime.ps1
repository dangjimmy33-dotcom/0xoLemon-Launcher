[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Container })]
    [string]$SourceRoot,

    [string]$DestinationRoot = (Join-Path $PSScriptRoot '..\native\steam_emu')
)

$ErrorActionPreference = 'Stop'
$source = (Resolve-Path -LiteralPath $SourceRoot).Path.TrimEnd('\')
$destination = [System.IO.Path]::GetFullPath($DestinationRoot).TrimEnd('\')

if ($source -eq $destination) {
    throw 'SourceRoot and DestinationRoot must be different directories.'
}

$excludedTopLevel = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
)
[void]$excludedTopLevel.Add('build')
[void]$excludedTopLevel.Add('BUILD_GSE_FULL_V6.bat')

[System.IO.Directory]::CreateDirectory($destination) | Out-Null

$files = Get-ChildItem -LiteralPath $source -File -Recurse |
    Where-Object {
        $relative = $_.FullName.Substring($source.Length).TrimStart('\')
        $topLevel = $relative.Split('\', 2)[0]
        -not $excludedTopLevel.Contains($topLevel)
    } |
    Sort-Object { $_.FullName.Substring($source.Length).TrimStart('\') }

$manifestLines = [System.Collections.Generic.List[string]]::new()
$copied = 0
$unchanged = 0

foreach ($file in $files) {
    $relative = $file.FullName.Substring($source.Length).TrimStart('\')
    $target = Join-Path $destination $relative
    $targetParent = Split-Path -Parent $target
    [System.IO.Directory]::CreateDirectory($targetParent) | Out-Null

    $sourceHash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $targetMatches = $false
    if (Test-Path -LiteralPath $target -PathType Leaf) {
        $targetHash = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant()
        $targetMatches = $targetHash -eq $sourceHash
    }

    if ($targetMatches) {
        $unchanged++
    }
    else {
        Copy-Item -LiteralPath $file.FullName -Destination $target -Force
        $copied++
    }

    $manifestLines.Add("$sourceHash  $($relative.Replace('\', '/'))")
}

$manifestPath = Join-Path $destination 'SOURCE_MANIFEST.sha256'
$manifestText = ($manifestLines -join "`n") + "`n"
[System.IO.File]::WriteAllText($manifestPath, $manifestText, [System.Text.UTF8Encoding]::new($false))
$manifestHash = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()

$provenance = [ordered]@{
    schemaVersion = 1
    upstream = 'Detanup01/gbe_fork'
    importedFrom = $source
    importedAt = (Get-Date).ToUniversalTime().ToString('o')
    license = 'LGPL-3.0'
    sourceFileCount = $files.Count
    sourceManifest = 'SOURCE_MANIFEST.sha256'
    sourceManifestSha256 = $manifestHash
    exclusions = @(
        'build/ (generated build output)'
        'BUILD_GSE_FULL_V6.bat (machine-local helper with unsafe cleanup commands)'
    )
    synchronization = 'Overlay copy. Existing dependency cache files not present in the snapshot are retained and are not part of the source manifest.'
}
$provenancePath = Join-Path $destination 'SOURCE_PROVENANCE.json'
$provenanceJson = $provenance | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText($provenancePath, $provenanceJson + "`n", [System.Text.UTF8Encoding]::new($false))

[pscustomobject]@{
    SourceFiles = $files.Count
    CopiedFiles = $copied
    UnchangedFiles = $unchanged
    ManifestSha256 = $manifestHash
    Destination = $destination
}
