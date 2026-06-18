# ============================================================
# steam-fetch-game-assets.ps1
# Fetch ALL game assets from Steam matching the launcher's
# expected directory structure (same as Among Us).
# ============================================================

param(
    [Parameter(Mandatory=$true)]  [string]$AppId,
    [Parameter(Mandatory=$true)]  [string]$GameName,
    [string]$AssetsRoot         = "E:\007Launcher\src\assets",
    [switch]$NoAchievementImages,
    [int]$MaxScreenshots        = 10,
    [int]$MaxVideos             = 5,
    [int]$ScreenshotQuality     = 55,
    [int]$ImageQuality          = 75,
    [int]$VideoCrf              = 35,
    [string]$VideoScale         = "720",
    [string]$SteamApiKey        = "",
    [string]$SteamGridDbKey     = "",
    [string]$Language           = "english",
    [string]$SteamCountry       = "us",
    [switch]$Overwrite,
    [switch]$CookMetadataAssets,
    [switch]$SkipRootImages,
    [switch]$PreserveRootAssets
)

$ErrorActionPreference = "Continue"

# ---- Helpers ----

function Slugify($str) {
    if (-not $str) { return "unknown" }
    ($str -replace '[^\w\s-]', '' -replace '\s+', '-' -replace '-+', '-').ToLower().Trim('-')
}

function Get-FileSha256($path) {
    (Get-FileHash -Path $path -Algorithm SHA256).Hash.ToLower()
}

function Test-ValidFile($path, [int64]$MinBytes = 64) {
    if (-not (Test-Path $path)) { return $false }
    try {
        $item = Get-Item $path -ErrorAction Stop
        return ($item.Length -ge $MinBytes)
    } catch {
        return $false
    }
}



function Test-VideoDecodeClean($path) {
    $ffmpeg = Get-Command ffmpeg -ErrorAction SilentlyContinue
    if (-not $ffmpeg) { return $true }
    try {
        $nullOut = if ($env:OS -like "*Windows*" -or $PSVersionTable.Platform -eq "Win32NT") { "NUL" } else { "/dev/null" }
        $errs = & $ffmpeg.Source -hide_banner -v error -i "$path" -map 0:v:0 -f null $nullOut 2>&1
        if ($LASTEXITCODE -ne 0) { return $false }
        if ($errs -and (($errs | Out-String).Trim().Length -gt 0)) { return $false }
        return $true
    } catch {
        return $false
    }
}

function Test-ValidVideo($path) {
    # Dung lượng thôi chưa đủ: bản cũ có thể để lại MP4 hỏng nhưng >1KB,
    # asset_pack_builder/ffmpeg sẽ báo "Invalid NAL unit size" khi build.
    if (-not (Test-ValidFile $path 16384)) { return $false }

    $ffprobe = Get-Command ffprobe -ErrorAction SilentlyContinue
    if ($ffprobe) {
        try {
            $durationText = & $ffprobe.Source -v error -select_streams v:0 -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 "$path" 2>$null
            if ($LASTEXITCODE -ne 0) { return $false }
            $duration = 0.0
            if ([double]::TryParse(($durationText | Select-Object -First 1), [System.Globalization.NumberStyles]::Float, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$duration)) {
                if ($duration -le 0.2) { return $false }
                return (Test-VideoDecodeClean $path)
            }
            return $false
        } catch {
            return $false
        }
    }

    # Nếu thiếu ffprobe thì fallback sang kiểm tra size để tool vẫn chạy được.
    return $true
}

function Remove-BadFile($path) {
    if (Test-Path $path) {
        try { Remove-Item $path -Force -ErrorAction SilentlyContinue } catch {}
    }
}

function Download-File($url, $outPath) {
    if (-not $url -or $url.Trim() -eq "") { return $false }
    if (Test-Path $outPath) {
        if ((Test-ValidFile $outPath 64) -and (-not $Overwrite)) {
            Write-Host "    [skip] $(Split-Path -Leaf $outPath)" -ForegroundColor DarkGray
            return $true
        }
        Remove-BadFile $outPath
    }
    try {
        Invoke-WebRequest -Uri $url -OutFile $outPath -UseBasicParsing -TimeoutSec 60
        if (Test-ValidFile $outPath 64) { return $true }
        Remove-BadFile $outPath
        Write-Warning "    Download returned empty/corrupt file: $url"
        return $false
    } catch {
        Write-Warning "    Download failed: $url"
        Remove-BadFile $outPath
        return $false
    }
}

function Convert-ToWebP($inPath, $outPath, [int]$quality = 75) {
    if (-not (Test-ValidFile $inPath 64)) { return $false }
    Remove-BadFile $outPath
    & ffmpeg -hide_banner -loglevel error -i $inPath -c:v libwebp -q:v $quality -y $outPath 2>&1 | Out-Null
    if ($LASTEXITCODE -eq 0 -and (Test-ValidFile $outPath 64)) { return $true }
    Remove-BadFile $outPath
    return $false
}

function Download-And-Convert-WebP($url, $outPath, [int]$quality = 75, [bool]$WarnOnFail = $true) {
    if (-not $url -or $url.Trim() -eq "") { return $false }

    if (Test-Path $outPath) {
        if ((Test-ValidFile $outPath 64) -and (-not $Overwrite)) {
            Write-Host "    [skip] $(Split-Path -Leaf $outPath)" -ForegroundColor DarkGray
            return $true
        }
        # Hotfix: file 0 byte/corrupt from an earlier failed ffmpeg run must not be treated as success.
        Write-Host "    [repair] $(Split-Path -Leaf $outPath) bị rỗng/hỏng, tải lại..." -ForegroundColor DarkYellow
        Remove-BadFile $outPath
    }

    $cleanUrl = ($url -split '\?')[0]
    $ext = if ($cleanUrl -match '\.png$') { ".png" } elseif ($cleanUrl -match '\.webp$') { ".webp" } elseif ($cleanUrl -match '\.avif$') { ".avif" } elseif ($cleanUrl -match '\.ico$') { ".ico" } else { ".jpg" }
    $tmp = "$outPath.tmp$ext"
    Remove-BadFile $tmp

    try {
        Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing -TimeoutSec 60
        if (-not (Test-ValidFile $tmp 64)) {
            Remove-BadFile $tmp
            if ($WarnOnFail) { Write-Warning "    Download returned empty file: $url" }
            return $false
        }
    } catch {
        if ($WarnOnFail) { Write-Warning "    Download failed: $url - $($_.Exception.Message)" }
        Remove-BadFile $tmp
        return $false
    }

    & ffmpeg -hide_banner -loglevel error -i $tmp -c:v libwebp -q:v $quality -y $outPath 2>&1 | Out-Null
    $ok = ($LASTEXITCODE -eq 0 -and (Test-ValidFile $outPath 64))
    Remove-BadFile $tmp

    if ($ok) { return $true }

    Remove-BadFile $outPath
    if ($WarnOnFail) { Write-Warning "    Convert failed or produced 0-byte WebP: $url" }
    return $false
}

function Get-PropValue($obj, [string]$name) {
    if (-not $obj) { return $null }
    $prop = $obj.PSObject.Properties[$name]
    if ($prop) { return $prop.Value }
    return $null
}

function Get-MovieSourceUrl($vid) {
    if (-not $vid) { return $null }

    $mp4 = Get-PropValue $vid "mp4"
    if ($mp4) {
        foreach ($k in @("max", "1080", "720", "480", "600")) {
            $u = Get-PropValue $mp4 $k
            if ($u) { return $u }
        }
    }

    $webm = Get-PropValue $vid "webm"
    if ($webm) {
        foreach ($k in @("max", "1080", "720", "480", "600")) {
            $u = Get-PropValue $webm $k
            if ($u) { return $u }
        }
    }

    $hls = Get-PropValue $vid "hls_h264"
    if ($hls) { return $hls }
    return $null
}

function Transcode-Video($sourceUrl, $outPath) {
    if (-not $sourceUrl -or $sourceUrl.Trim() -eq "") { return $false }
    if (Test-Path $outPath) {
        if ((Test-ValidVideo $outPath) -and (-not $Overwrite)) {
            Write-Host "    [skip] $(Split-Path -Leaf $outPath)" -ForegroundColor DarkGray
            return $true
        }
        Write-Host "    [repair] MP4 cũ bị rỗng/hỏng, tạo lại: $(Split-Path -Leaf $outPath)" -ForegroundColor DarkYellow
        Remove-BadFile $outPath
    }

    Write-Host "    Transcoding MP4/H.264... (có thể mất 1-2 phút)"

    $inputForFfmpeg = $sourceUrl
    $tmpVideo = $null
    $cleanUrl = ($sourceUrl -split '\?')[0]
    if ($cleanUrl -match '\.(mp4|webm|mov|m4v)$') {
        $ext = "." + $Matches[1]
        $tmpVideo = "$outPath.source$ext"
        Remove-BadFile $tmpVideo
        if (Download-File $sourceUrl $tmpVideo) {
            $inputForFfmpeg = $tmpVideo
        } else {
            $inputForFfmpeg = $sourceUrl
        }
    }

    $tmpOut = "$outPath.tmp.mp4"
    Remove-BadFile $tmpOut
    & ffmpeg -hide_banner -loglevel error -fflags +genpts+discardcorrupt -err_detect ignore_err -i $inputForFfmpeg -map 0:v:0 -map 0:a? -sn -dn -c:v libx264 -preset veryfast -crf $VideoCrf -vf "scale=-2:$VideoScale,fps=24" -pix_fmt yuv420p -profile:v main -level 4.0 -movflags +faststart -c:a aac -b:a 96k -y $tmpOut 2>&1 | Out-Null
    $ffOk = ($LASTEXITCODE -eq 0)
    if ($tmpVideo) { Remove-BadFile $tmpVideo }

    if ($ffOk -and (Test-ValidVideo $tmpOut)) {
        Move-Item -Force $tmpOut $outPath
        return $true
    }
    Remove-BadFile $tmpOut
    Remove-BadFile $outPath
    Write-Warning "    Transcode failed or MP4 còn cảnh báo decode; video này sẽ bị bỏ khỏi manifest để tránh lỗi build."
    return $false
}

function Make-MediaEntry($role, $title, $sourceUrl, $filePath, $relFile) {
    if ($role -eq "video") {
        if (-not (Test-ValidVideo $filePath)) { return $null }
    } else {
        if (-not (Test-ValidFile $filePath 64)) { return $null }
    }
    return [ordered]@{
        role             = $role
        title            = $title
        sourceUrl        = $sourceUrl
        file             = $relFile
        sizeBytes        = [long](Get-Item $filePath).Length
        sha256           = Get-FileSha256 $filePath
        lastWriteTimeUtc = (Get-Item $filePath).LastWriteTimeUtc.ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
    }
}


function Make-RemoteMediaEntry($role, $title, $sourceUrl, $relFile) {
    if (-not $sourceUrl -or $sourceUrl.Trim() -eq "") { return $null }
    return [ordered]@{
        role             = $role
        title            = $title
        sourceUrl        = $sourceUrl
        file             = $relFile
        remoteOnly       = $true
        sizeBytes        = 0
        sha256           = ""
        lastWriteTimeUtc = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
    }
}

function Extract-DescriptionImageUrls($html) {
    if (-not $html) { return @() }
    $pattern = 'https?://[^\s"]+?/extras/([a-f0-9]+)\.(avif|jpg|png|webp)[^"]*'
    $matches = [regex]::Matches($html, $pattern)
    $seen = @{}; $result = @()
    foreach ($m in $matches) {
        $hash = $m.Groups[1].Value
        if (-not $seen.ContainsKey($hash)) {
            $seen[$hash] = $true
            $result += [pscustomobject]@{ Url = $m.Value; Hash = $hash }
        }
    }
    return $result
}

function Invoke-SGDB($url, $label) {
    try {
        return Invoke-RestMethod -Uri $url -Headers @{ "Authorization" = "Bearer $SteamGridDbKey"; "Accept" = "application/json" } -UseBasicParsing -TimeoutSec 25
    } catch {
        Write-Host "    [sgdb skip] ${label}: $($_.Exception.Message)" -ForegroundColor DarkYellow
        return $null
    }
}

function Get-SGDB-Assets($type, $appId) {
    if (-not $SteamGridDbKey) { return @() }
    $encodedAppId = [System.Uri]::EscapeDataString([string]$appId)

    # Đúng endpoint SteamGridDB hiện tại: /grids/steam/{appid}, /heroes/steam/{appid}, ...
    $platformUrl = "https://www.steamgriddb.com/api/v2/$type/steam/$encodedAppId"
    $res = Invoke-SGDB $platformUrl "$type/steam/$appId"
    if ($res -and $res.success -and $res.data) { return @($res.data) }

    # Fallback cũ: lấy internal SGDB game id rồi gọi /{type}/game/{gameId}
    $gameRes = Invoke-SGDB "https://www.steamgriddb.com/api/v2/games/steam/$encodedAppId" "games/steam/$appId"
    $gameId = $null
    if ($gameRes -and $gameRes.success -and $gameRes.data) {
        $gameId = $gameRes.data.id
    }
    if ($gameId) {
        $byGameUrl = "https://www.steamgriddb.com/api/v2/$type/game/$gameId"
        $res2 = Invoke-SGDB $byGameUrl "$type/game/$gameId"
        if ($res2 -and $res2.success -and $res2.data) { return @($res2.data) }
    }
    return @()
}

# ---- Setup dirs ----
$slug = Slugify $GameName
$gameDir     = Join-Path $AssetsRoot $GameName
$detailsDir  = Join-Path $gameDir "details"
$metaDir     = Join-Path $detailsDir "metadata"
$ssDir       = Join-Path $detailsDir "screenshots"
$vidDir      = Join-Path $detailsDir "videos"
$thumbDir    = Join-Path $detailsDir "video_thumbnails"
$storeDir    = Join-Path $detailsDir "store_art"
$descDir     = Join-Path $detailsDir "description_images"
$achievDir   = Join-Path $gameDir "achievement_images"

foreach ($d in @($gameDir,$detailsDir,$metaDir,$ssDir,$vidDir,$thumbDir,$storeDir,$descDir,$achievDir)) {
    New-Item -ItemType Directory -Force -Path $d | Out-Null
}


function Normalize-SteamCountry($cc) {
    if (-not $cc) { return "us" }
    $v = ([string]$cc).Trim().ToLowerInvariant()
    if ($v.Length -ne 2) { return "us" }
    return $v
}

function Get-SteamStoreHeaders($lang) {
    $acceptLang = if ($lang -and $lang -ne "english") { "$lang,en-US;q=0.9,en;q=0.8" } else { "en-US,en;q=0.9" }
    return @{
        "User-Agent" = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 0xoLemonAssetBuilder/1.0"
        "Accept" = "application/json,text/plain,*/*"
        "Accept-Language" = $acceptLang
        # Some Steam appdetails pages are age gated. These cookies keep metadata fetch non-interactive.
        "Cookie" = "birthtime=568022401; lastagecheckage=1-January-1988; mature_content=1"
    }
}

function Get-SteamAppDetailsWithFallback($appid, $language, $preferredCountry) {
    $preferredCountry = Normalize-SteamCountry $preferredCountry
    $countries = New-Object System.Collections.Generic.List[string]
    foreach ($cc in @($preferredCountry, "us", "sg", "gb", "jp", "kr", "tw", "hk", "th", "vn", "de", "fr", "ca", "au")) {
        $n = Normalize-SteamCountry $cc
        if (-not $countries.Contains($n)) { $countries.Add($n) }
    }

    $languages = New-Object System.Collections.Generic.List[string]
    if ($language) { $languages.Add([string]$language) }
    if (-not $languages.Contains("english")) { $languages.Add("english") }

    $lastError = ""
    foreach ($lang in $languages) {
        foreach ($cc in $countries) {
            try {
                $query = "appids=$appid&l=$([uri]::EscapeDataString($lang))&cc=$cc"
                $url = "https://store.steampowered.com/api/appdetails?$query"
                Write-Host "  Try AppDetails: cc=${cc}, l=${lang}" -ForegroundColor DarkGray
                $headers = Get-SteamStoreHeaders $lang
                $rawTry = Invoke-RestMethod -Uri $url -Headers $headers -UseBasicParsing -TimeoutSec 35
                $entry = $rawTry.$appid
                if ($entry -and $entry.success -and $entry.data) {
                    return [pscustomobject]@{
                        Raw      = $rawTry
                        Data     = $entry.data
                        Country  = $cc
                        Language = $lang
                    }
                }
                $lastError = "cc=${cc} l=${lang}: success=false or no data"
            } catch {
                # Important: ${cc}/${lang} avoids PowerShell parsing $cc: as a scoped variable.
                $lastError = "cc=${cc} l=${lang}: $($_.Exception.Message)"
            }
        }
    }
    throw "Steam AppDetails returned no data for AppID ${appid}. Last error: $lastError"
}

# ---- Fetch Steam API ----
Write-Host "`n[1/8] Fetching Steam API (AppDetails, region fallback)..." -ForegroundColor Cyan
try {
    $detailsResult = Get-SteamAppDetailsWithFallback $AppId $Language $SteamCountry
    $raw = $detailsResult.Raw
    $data = $detailsResult.Data
    $SteamCountry = $detailsResult.Country
    $Language = $detailsResult.Language
} catch {
    Write-Error $_.Exception.Message
    exit 1
}
if (-not $data) { Write-Error "Steam API returned no data for AppID $AppId"; exit 1 }
$raw | ConvertTo-Json -Depth 20 | Set-Content "$metaDir\steam-appdetails.raw.json" -Encoding UTF8
Write-Host "  AppID: $($data.steam_appid), Title: $($data.name), cc=${SteamCountry}, l=${Language}"

# ---- Fetch Reviews & News ----
Write-Host "`n[2/8] Fetching Reviews & News..." -ForegroundColor Cyan
$reviewsData = $null
try {
    $revRaw = Invoke-RestMethod -Uri "https://store.steampowered.com/appreviews/$AppId?json=1&language=all" -UseBasicParsing
    $reviewsData = $revRaw.query_summary
    $revRaw | ConvertTo-Json -Depth 20 | Set-Content "$metaDir\steam-reviews.raw.json" -Encoding UTF8
    Write-Host "  Reviews: $($reviewsData.review_score_desc) ($($reviewsData.total_reviews) total)"
} catch {
    Write-Warning "  Failed to fetch reviews"
}

$newsData = @()
try {
    $newsRaw = Invoke-RestMethod -Uri "https://api.steampowered.com/ISteamNews/GetNewsForApp/v0002/?appid=$AppId&count=5&format=json" -UseBasicParsing
    $newsData = $newsRaw.appnews.newsitems
    $newsRaw | ConvertTo-Json -Depth 20 | Set-Content "$metaDir\steam-news.raw.json" -Encoding UTF8
    Write-Host "  News: Fetched $($newsData.Count) items"
} catch {
    Write-Warning "  Failed to fetch news"
}

# ---- Fetch Achievements ----
Write-Host "`n[3/8] Fetching Achievements Schema..." -ForegroundColor Cyan
$achievData = @()
try {
    $url = "https://api.steampowered.com/ISteamUserStats/GetSchemaForGame/v2/?appid=$AppId&l=$Language"
    if ($SteamApiKey) { $url += "&key=$SteamApiKey" }
    $achievRaw = Invoke-RestMethod -Uri $url -UseBasicParsing
    $achievData = $achievRaw.game.availableGameStats.achievements
    $achievRaw | ConvertTo-Json -Depth 20 | Set-Content "$metaDir\steam-achievements.raw.json" -Encoding UTF8
    Write-Host "  Found $($achievData.Count) achievements"
} catch {
    Write-Warning "  Achievements not available (no API key or no achievements)"
    @() | ConvertTo-Json | Set-Content "$metaDir\steam-achievements.raw.json" -Encoding UTF8
}

# ---- Get timestamp ----
$tMatch = [regex]::Match($data.header_image, '[?&]t=(\d+)')
$t = if ($tMatch.Success) { $tMatch.Groups[1].Value } else { "0" }

function Get-RootAssetExtRank($PathInfo) {
    $ext = $PathInfo.Extension.ToLowerInvariant()
    if ($ext -eq ".webp") { return 0 }
    if ($ext -eq ".png") { return 1 }
    if ($ext -eq ".jpg" -or $ext -eq ".jpeg") { return 2 }
    if ($ext -eq ".ico") { return 3 }
    return 9
}

function Find-ExistingRootAsset($Role) {
    try {
        $matches = Get-ChildItem -Path $gameDir -File -ErrorAction SilentlyContinue | Where-Object {
            $_.Name -match "^$([regex]::Escape($Role))[-_.].+\.(webp|png|jpg|jpeg|ico)$"
        }
        if ($matches) {
            $ranked = $matches | ForEach-Object {
                [pscustomobject]@{
                    File = $_
                    StemLen = $_.BaseName.Length
                    ExtRank = Get-RootAssetExtRank $_
                    NameKey = $_.Name.ToLowerInvariant()
                }
            } | Sort-Object StemLen, ExtRank, NameKey
            $first = $ranked | Select-Object -First 1
            if ($first) { return $first.File }
        }
    } catch {}
    return $null
}

# ---- Root images (SteamGridDB + Steam Fallback) ----
if ($SkipRootImages) {
    Write-Host "`n[4/8] Skip root images (grid/hero/logo/icon) - metadata-only mode." -ForegroundColor Cyan
} else {
Write-Host "`n[4/8] Downloading root images (grid/hero/logo/icon)..." -ForegroundColor Cyan
$rootUrls = [ordered]@{ "grid" = @(); "hero" = @(); "logo" = @(); "icon" = @() }

if ($SteamGridDbKey) {
    Write-Host "  Querying SteamGridDB for high-quality assets..." -ForegroundColor Yellow
    $sgdbGrids = Get-SGDB-Assets "grids" $AppId
    $sgdbHeroes = Get-SGDB-Assets "heroes" $AppId
    $sgdbLogos = Get-SGDB-Assets "logos" $AppId
    $sgdbIcons = Get-SGDB-Assets "icons" $AppId

    if ($sgdbGrids) { $rootUrls["grid"] += $sgdbGrids[0].url }
    if ($sgdbHeroes) { $rootUrls["hero"] += $sgdbHeroes[0].url }
    if ($sgdbLogos) { $rootUrls["logo"] += $sgdbLogos[0].url }
    if ($sgdbIcons) { $rootUrls["icon"] += $sgdbIcons[0].url }
}

# Steam Fallbacks
$rootUrls["grid"] += "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/$AppId/library_600x900.jpg?t=$t"
$rootUrls["grid"] += "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/$AppId/capsule_616x353.jpg?t=$t"

$rootUrls["hero"] += "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/$AppId/library_hero.jpg?t=$t"
if ($data.screenshots.Count -gt 0) {
    $rootUrls["hero"] += "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/$AppId/ss_$($data.screenshots[0].id).1920x1080.jpg?t=$t"
}

$rootUrls["logo"] += "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/$AppId/logo.png?t=$t"
$rootUrls["logo"] += "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/$AppId/logo_2x.png?t=$t"

$rootUrls["icon"] += "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/$AppId/capsule_231x87.jpg?t=$t"
$rootUrls["icon"] += $data.header_image

foreach ($role in $rootUrls.Keys) {
    $existing = Find-ExistingRootAsset $role
    if ($PreserveRootAssets -and $existing -and (-not $Overwrite)) {
        Write-Host "  [keep] $($existing.Name)" -ForegroundColor DarkGray
        continue
    }

    $outPath = Join-Path $gameDir "$role-$slug.webp"
    $qual = if ($role -eq "grid") { 85 } else { $ImageQuality }
    Write-Host "  $role-$slug.webp"
    $done = $false
    foreach ($url in $rootUrls[$role]) {
        if (Download-And-Convert-WebP $url $outPath $qual $false) { $done = $true; break }
    }
    if (-not $done) { Write-Warning "  Could not fetch $role" }
}
}

# ---- Store art ----
Write-Host "`n[5/8] Writing remote store-art metadata..." -ForegroundColor Cyan
$storeArtDef = [ordered]@{
    "header"         = @{ url = "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/$AppId/header.jpg?t=$t"; title = "Steam header" }
    "capsule"        = @{ url = "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/$AppId/capsule_231x87.jpg?t=$t"; title = "Steam capsule" }
    "capsule-v5"     = @{ url = "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/$AppId/capsule_184x69.jpg?t=$t"; title = "Steam capsule v5" }
    "background"     = @{ url = "https://store.akamai.steamstatic.com/images/storepagebackground/app/${AppId}?t=$t"; title = "Steam background" }
    "background-raw" = @{ url = "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/$AppId/page_bg_raw.jpg?t=$t"; title = "Steam background raw" }
}
$mediaManifest = [System.Collections.Generic.List[object]]::new()
foreach ($key in $storeArtDef.Keys) {
    $def = $storeArtDef[$key]
    if ($CookMetadataAssets) {
        $outPath = Join-Path $storeDir "$key.webp"
        Write-Host "  cook store_art/$key.webp"
        $warnStoreArt = -not ($key -in @("background-raw"))
        $okStoreArt = Download-And-Convert-WebP $def.url $outPath $ImageQuality $warnStoreArt
        if (-not $okStoreArt -and -not $warnStoreArt) { Write-Host "    [optional skip] $key không có trên Steam CDN" -ForegroundColor DarkGray }
        $e = Make-MediaEntry $key $def.title $def.url $outPath "store_art/$key.webp"
        if ($e) { $mediaManifest.Add($e) }
    } else {
        Write-Host "  remote store_art/$key" -ForegroundColor DarkGray
        $e = Make-RemoteMediaEntry $key $def.title $def.url "store_art/$key.webp"
        if ($e) { $mediaManifest.Add($e) }
    }
}

# ---- Description images ----
Write-Host "`n[6/8] Writing remote description-image metadata..." -ForegroundColor Cyan
$descHtml = $data.about_the_game
$descUrls = Extract-DescriptionImageUrls $descHtml
$descImageMap = @{}
$di = 1
foreach ($entry in $descUrls) {
    $pad = $di.ToString("D3")
    $rel = "description_images/${pad}_$($entry.Hash).webp"
    if ($CookMetadataAssets) {
        $outPath = Join-Path $descDir "${pad}_$($entry.Hash).webp"
        Write-Host "  cook Description img $di : $($entry.Hash)"
        if (Download-And-Convert-WebP $entry.Url $outPath $ImageQuality) {
            $descImageMap[$entry.Url] = "asset:$rel"
        }
    } else {
        Write-Host "  remote Description img $di : $($entry.Hash)" -ForegroundColor DarkGray
        $descImageMap[$entry.Url] = "asset:$rel"
    }
    $e = Make-RemoteMediaEntry "description-image" "Description image $di" $entry.Url $rel
    if ($e) { $mediaManifest.Add($e) }
    $di++
}

# Rewrite HTML through stable local tokens. The Rust packer maps these tokens to remote64 URLs.
$processedHtml = $descHtml
foreach ($k in $descImageMap.Keys) {
    $processedHtml = $processedHtml.Replace($k, $descImageMap[$k])
}

# ---- Screenshots ----
Write-Host "`n[7/8] Writing remote screenshot metadata..." -ForegroundColor Cyan
$ssCount = [Math]::Min($MaxScreenshots, $data.screenshots.Count)
for ($i = 0; $i -lt $ssCount; $i++) {
    $ss = $data.screenshots[$i]
    $pad = ($i+1).ToString("D3")
    $rel = "screenshots/${pad}_screenshot.webp"
    Write-Host "  Screenshot $($i+1)/$ssCount"
    if ($CookMetadataAssets) {
        $outPath = Join-Path $ssDir "${pad}_screenshot.webp"
        Download-And-Convert-WebP $ss.path_full $outPath $ScreenshotQuality | Out-Null
        $e = Make-MediaEntry "screenshot" "Screenshot $($i+1)" $ss.path_full $outPath $rel
    } else {
        $e = Make-RemoteMediaEntry "screenshot" "Screenshot $($i+1)" $ss.path_full $rel
    }
    if ($e) { $mediaManifest.Add($e) }
}

# ---- Videos ----
Write-Host "`n[8/8] Writing remote video & achievement metadata..." -ForegroundColor Cyan
$vidCount = [Math]::Min($MaxVideos, $data.movies.Count)
for ($i = 0; $i -lt $vidCount; $i++) {
    $vid = $data.movies[$i]
    $pad = ($i+1).ToString("D3")
    $nameSlug = Slugify $vid.name
    Write-Host "  Video $($i+1)/$vidCount : $($vid.name)"

    $thumbUrl  = $vid.thumbnail
    $posterUrl = $thumbUrl -replace 'movie_\d+x\d+\.jpg', 'movie_600x337.jpg' -replace 'movie\.\d+x\d+\.jpg', 'movie_600x337.jpg'
    if ($posterUrl -eq $thumbUrl) { $posterUrl = $thumbUrl -replace 'movie\.\d+x\d+', 'movie.600x337' }
    $videoUrl  = Get-MovieSourceUrl $vid

    $videoRel = "videos/${pad}_${nameSlug}.mp4"
    $thumbRel = "video_thumbnails/${pad}_${nameSlug}_thumb.webp"
    $posterRel = "video_thumbnails/${pad}_${nameSlug}_poster.webp"

    if ($CookMetadataAssets) {
        $thumbPath = Join-Path $thumbDir "${pad}_${nameSlug}_thumb.webp"
        Download-And-Convert-WebP $thumbUrl $thumbPath $ImageQuality | Out-Null
        $posterPath = Join-Path $thumbDir "${pad}_${nameSlug}_poster.webp"
        $posterOk = Download-And-Convert-WebP $posterUrl $posterPath $ImageQuality $false
        if (-not $posterOk -and (Test-Path $thumbPath)) { Copy-Item $thumbPath $posterPath -Force }
        $videoPath = Join-Path $vidDir "${pad}_${nameSlug}.mp4"
        if ($videoUrl) { Transcode-Video $videoUrl $videoPath | Out-Null }
        $vE = Make-MediaEntry "video" $vid.name $videoUrl $videoPath $videoRel
        $tE = Make-MediaEntry "video-thumbnail" $vid.name $thumbUrl $thumbPath $thumbRel
        $pE = Make-MediaEntry "video-poster" $vid.name $posterUrl $posterPath $posterRel
    } else {
        $vE = Make-RemoteMediaEntry "video" $vid.name $videoUrl $videoRel
        $tE = Make-RemoteMediaEntry "video-thumbnail" $vid.name $thumbUrl $thumbRel
        $pE = Make-RemoteMediaEntry "video-poster" $vid.name $posterUrl $posterRel
    }
    if ($vE) { $mediaManifest.Add($vE) }
    if ($tE) { $mediaManifest.Add($tE) }
    if ($pE) { $mediaManifest.Add($pE) }
}
$mediaManifest | ConvertTo-Json -Depth 10 | Set-Content "$metaDir\media-manifest.json" -Encoding UTF8

# Processing Achievements
$achievItems = @()
if ($achievData -and $achievData.Count -gt 0) {
    Write-Host "  Processing achievements images..."
    $ai = 1
    foreach ($ach in $achievData) {
        $pad = $ai.ToString("D3")
        $nameSlug = Slugify $ach.name
        $iconFile = ""
        $iconGrayFile = ""

        if ($CookMetadataAssets -and -not $NoAchievementImages) {
            if ($ach.icon) {
                $iconPath = Join-Path $achievDir "${pad}_${nameSlug}.jpg"
                $iconFile = "achievement_images/${pad}_${nameSlug}.jpg"
                if (-not (Test-ValidFile $iconPath 64) -or $Overwrite) {
                    Download-File $ach.icon $iconPath | Out-Null
                }
                if (-not (Test-ValidFile $iconPath 64)) { $iconFile = "" }
            }
            if ($ach.icongray) {
                $grayPath = Join-Path $achievDir "${pad}_${nameSlug}_locked.jpg"
                $iconGrayFile = "achievement_images/${pad}_${nameSlug}_locked.jpg"
                if (-not (Test-ValidFile $grayPath 64) -or $Overwrite) {
                    Download-File $ach.icongray $grayPath | Out-Null
                }
                if (-not (Test-ValidFile $grayPath 64)) { $iconGrayFile = "" }
            }
        }
        $achievItems += [ordered]@{
            apiName       = $ach.name
            displayName   = $ach.displayName
            description   = if ($ach.description) { $ach.description } else { "" }
            hidden        = ($ach.hidden -eq 1)
            iconFile      = $iconFile
            iconGrayFile  = $iconGrayFile
            steamIcon     = $ach.icon
            steamIconGray = $ach.icongray
        }
        $ai++
    }
}

# ---- Build game-detail.normalized.json ----
Write-Host "`n  Building extended game-detail.normalized.json..."
$genres     = if ($data.genres) { ($data.genres | ForEach-Object { $_.description }) -join ", " } else { "" }
$categories = if ($data.categories) { @($data.categories | ForEach-Object { $_.description }) } else { @() }
$devs       = if ($data.developers.Count -eq 1) { $data.developers[0] } else { $data.developers }
$pubs       = if ($data.publishers.Count -eq 1) { $data.publishers[0] } else { $data.publishers }

$normalized = [ordered]@{
    appId                   = [int]$data.steam_appid
    title                   = $GameName
    steamMetadataCountry    = $SteamCountry
    steamMetadataLanguage   = $Language
    type                    = $data.type
    requiredAge             = $data.required_age
    isFree                  = $data.is_free
    shortDescription        = $data.short_description
    detailedDescriptionHtml = $processedHtml
    aboutTheGameHtml        = $processedHtml
    supportedLanguagesHtml  = $data.supported_languages
    pcRequirements          = $data.pc_requirements
    macRequirements         = $data.mac_requirements
    linuxRequirements       = $data.linux_requirements
    dlc                     = $data.dlc
    website                 = $data.website
    developers              = $devs
    publishers              = $pubs
    releaseDate             = $data.release_date
    genres                  = $genres
    categories              = $categories
    platforms               = $data.platforms
    metacritic              = $data.metacritic
    recommendations         = $data.recommendations
    reviewsSummary          = $reviewsData
    newsItems               = $newsData
    achievements            = [ordered]@{
        total = [int]$(if ($achievItems.Count -gt 0) { $achievItems.Count } elseif ($data.achievements) { $data.achievements.total } else { 0 })
        items = $achievItems
    }
}
$normalized | ConvertTo-Json -Depth 20 | Set-Content "$metaDir\game-detail.normalized.json" -Encoding UTF8

# ---- Summary ----
Write-Host "`n============================================================" -ForegroundColor Green
Write-Host "DONE: $GameName (AppID: $AppId)" -ForegroundColor Green
Write-Host "  Slug         : $slug"
Write-Host "  Screenshots  : $(if ($CookMetadataAssets) { Get-ChildItem $ssDir *.webp -ErrorAction SilentlyContinue | Measure-Object | Select -Expand Count } else { "remote metadata" })"
Write-Host "  Videos       : $(if ($CookMetadataAssets) { Get-ChildItem $vidDir *.mp4 -ErrorAction SilentlyContinue | Measure-Object | Select -Expand Count } else { "remote metadata" })"
Write-Host "  Achievements : $($achievItems.Count) (icons remote unless -CookMetadataAssets)"
Write-Host "  Reviews      : $(if ($reviewsData) { "Fetched" } else { "Failed" })"
Write-Host "  News         : $($newsData.Count)"
Write-Host "  Media entries: $($mediaManifest.Count)"
Write-Host "  Output       : $gameDir"
Write-Host "============================================================`n" -ForegroundColor Green
