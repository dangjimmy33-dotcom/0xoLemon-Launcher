param(
  [ValidateSet('Release','Debug')]
  [string]$Configuration = 'Release',
  [switch]$SkipIfPresent
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Source = Join-Path $Here 'vendor\cloudredirect'
$Build64 = Join-Path $Here 'target\cloudredirect-native-x64'
$Build32 = Join-Path $Here 'target\cloudredirect-cloud760-x86'
$Destination = Join-Path $Here 'resources\cloud_redirect\engine\2.6.4'
$Required = @('0xoCloudRedirect.dll', 'cloud_redirect_cli.exe', 'cloud760_tool.exe')

if ($SkipIfPresent) {
  $allPresent = $true
  foreach ($name in $Required) {
    if (-not (Test-Path (Join-Path $Destination $name))) { $allPresent = $false; break }
  }
  if ($allPresent) {
    Write-Host '[CloudRedirect] Native engine already present; skipping build.'
    exit 0
  }
}

if (-not (Test-Path (Join-Path $Source 'CMakeLists.txt'))) {
  throw "CloudRedirect source not found: $Source"
}
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
  throw 'CMake is required to build CloudRedirect 2.6.4.'
}

New-Item -ItemType Directory -Force -Path $Build64, $Build32, $Destination | Out-Null
Write-Host '[CloudRedirect] Configuring the x64 DLL and JSON CLI...'
cmake -S $Source -B $Build64 -A x64 -DBUILD_TESTING=OFF
if ($LASTEXITCODE -ne 0) { throw "CloudRedirect x64 configure failed with exit code $LASTEXITCODE" }
cmake --build $Build64 --config $Configuration --target cloud_redirect cloud_redirect_cli --parallel
if ($LASTEXITCODE -ne 0) { throw "CloudRedirect x64 build failed with exit code $LASTEXITCODE" }

Write-Host '[CloudRedirect] Configuring the x86 Cloud760 utility...'
cmake -S $Source -B $Build32 -A Win32 -DBUILD_TESTING=OFF
if ($LASTEXITCODE -ne 0) { throw "CloudRedirect x86 configure failed with exit code $LASTEXITCODE" }
cmake --build $Build32 --config $Configuration --target cloud760_tool --parallel
if ($LASTEXITCODE -ne 0) { throw "CloudRedirect Cloud760 build failed with exit code $LASTEXITCODE" }

$Artifacts = @{
  '0xoCloudRedirect.dll'   = Join-Path (Join-Path $Build64 $Configuration) '0xoCloudRedirect.dll'
  'cloud_redirect_cli.exe' = Join-Path (Join-Path $Build64 $Configuration) 'cloud_redirect_cli.exe'
  'cloud760_tool.exe'      = Join-Path (Join-Path $Build32 $Configuration) 'cloud760_tool.exe'
}
foreach ($name in $Artifacts.Keys) {
  $sourceFile = $Artifacts[$name]
  if (-not (Test-Path $sourceFile)) { throw "Expected CloudRedirect artifact missing: $sourceFile" }
  Copy-Item -Force $sourceFile (Join-Path $Destination $name)
}

# The upstream release UI embeds a 32-bit steam_api.dll for Cloud760. A source
# archive may omit that binary. Accept an explicit release-asset path or copy it
# from the vendored tree when present; Cloud760 stays disabled if neither exists.
$SteamApiCandidates = @()
if ($env:CLOUDREDIRECT_STEAM_API_DLL) { $SteamApiCandidates += $env:CLOUDREDIRECT_STEAM_API_DLL }
$SteamApiCandidates += @(
  (Join-Path $Source 'ui\native\steam_api.dll'),
  (Join-Path $Source 'steam_api.dll'),
  # 32-bit steam_api.dll from local Steamworks SDK (not redistributed in repo)
  'E:\Compressed\steamworks_sdk_165\sdk\redistributable_bin\steam_api.dll',
  # Fallback: grab from Steam client installation
  (& { $p = ${env:ProgramFiles(x86)}; if ($p) { Join-Path $p 'Steam\steam_api.dll' } })
)
$SteamApiCopied = $false
foreach ($candidate in $SteamApiCandidates) {
  if ($candidate -and (Test-Path $candidate)) {
    Copy-Item -Force $candidate (Join-Path $Destination 'steam_api.dll')
    $SteamApiCopied = $true
    break
  }
}
if (-not $SteamApiCopied) {
  Write-Warning '[CloudRedirect] steam_api.dll was not present. Engine features work, but the optional Cloud760 tool will remain unavailable.'
}

$versionProps = Get-Content (Join-Path $Source 'Version.props') -Raw
$version = if ($versionProps -match '<ReleaseVersion>([^<]+)</ReleaseVersion>') { $Matches[1] } else { 'unknown' }
$commit = '9d0dbbf48f349a4172d2d47a936bb41c5f5ecff6'
@{
  version = $version
  commit = $commit
  builtAtUtc = [DateTime]::UtcNow.ToString('o')
  configuration = $Configuration
  cloud760SteamApiBundled = $SteamApiCopied
} | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Destination 'engine.json')

Write-Host "[CloudRedirect] Engine $version ready at $Destination"
