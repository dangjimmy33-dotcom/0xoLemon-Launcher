[CmdletBinding()]
param(
    [string]$SourceRoot = (Join-Path $PSScriptRoot '..\native\steam_emu'),
    [string]$MsBuildPath = 'E:\visual studio 2022\MSBuild\Current\Bin\MSBuild.exe',
    [switch]$Publish
)

$ErrorActionPreference = 'Stop'
$source = (Resolve-Path -LiteralPath $SourceRoot).Path
$msbuild = (Resolve-Path -LiteralPath $MsBuildPath).Path
$solution = Join-Path $source 'build\project\vs2022\win\gse.sln'

if (-not (Test-Path -LiteralPath $solution -PathType Leaf)) {
    throw "Generate the VS2022 GSE solution first; expected: $solution"
}

$platforms = @(
    [pscustomobject]@{
        Platform = 'Win32'
        Output = 'build\win\vs2022\release\regular\x86\steam_api.dll'
        PublishTo = 'src-tauri\resources\emu\x86\steam_api.dll'
    },
    [pscustomobject]@{
        Platform = 'x64'
        Output = 'build\win\vs2022\release\regular\x64\steam_api64.dll'
        PublishTo = 'src-tauri\resources\emu\x64\steam_api64.dll'
    }
)

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$results = foreach ($entry in $platforms) {
    & $msbuild /nologo /m:1 /p:CL_MPCount=4 /v:minimal /p:Configuration=release "/p:Platform=$($entry.Platform)" /target:api_regular $solution
    if ($LASTEXITCODE -ne 0) {
        throw "GSE $($entry.Platform) build failed with exit code $LASTEXITCODE."
    }

    $output = Join-Path $source $entry.Output
    if (-not (Test-Path -LiteralPath $output -PathType Leaf)) {
        throw "GSE build did not produce $output"
    }

    $hash = (Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($Publish) {
        $publishPath = Join-Path $repoRoot $entry.PublishTo
        [System.IO.Directory]::CreateDirectory((Split-Path -Parent $publishPath)) | Out-Null
        Copy-Item -LiteralPath $output -Destination $publishPath -Force
        $publishedHash = (Get-FileHash -LiteralPath $publishPath -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($publishedHash -ne $hash) {
            throw "Published artifact hash mismatch for $publishPath"
        }
    }

    [pscustomobject]@{
        Platform = $entry.Platform
        Path = $output
        Sha256 = $hash
        Published = [bool]$Publish
    }
}

$results
