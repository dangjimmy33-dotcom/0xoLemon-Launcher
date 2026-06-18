$appId = "921570"
$baseDir = "E:\007Launcher\src\assets\OCTOPATH TRAVELER 0"
$metadataDir = "$baseDir\details\metadata"

if (-not (Test-Path $metadataDir)) { New-Item -ItemType Directory -Force -Path $metadataDir | Out-Null }
if (-not (Test-Path "$baseDir\details\screenshots")) { New-Item -ItemType Directory -Force -Path "$baseDir\details\screenshots" | Out-Null }
if (-not (Test-Path "$baseDir\details\videos")) { New-Item -ItemType Directory -Force -Path "$baseDir\details\videos" | Out-Null }
if (-not (Test-Path "$baseDir\details\video_thumbnails")) { New-Item -ItemType Directory -Force -Path "$baseDir\details\video_thumbnails" | Out-Null }

Write-Host "Fetching metadata..."
$response = Invoke-RestMethod -Uri "https://store.steampowered.com/api/appdetails?appids=$appId&l=english"
$data = $response.$appId.data

$normalized = @{
    title = "OCTOPATH TRAVELER 0"
    appId = $data.steam_appid
    developers = $data.developers
    publishers = $data.publishers
    metacritic = $data.metacritic
    shortDescription = $data.short_description
    detailedDescriptionHtml = $data.about_the_game
    supportedLanguages = $data.supported_languages
    pcRequirements = $data.pc_requirements
    legalNotice = $data.legal_notice
}
$normalized | ConvertTo-Json -Depth 10 | Out-File -FilePath "$metadataDir\game-detail.normalized.json" -Encoding UTF8

$mediaManifest = @()

$i = 0
foreach ($ss in $data.screenshots) {
    if ($i -ge 5) { break }
    $mediaManifest += @{
        role = "screenshot"
        title = "Screenshot $i"
        file = "screenshots/ss_$($ss.id).webp"
    }

    $origPath = "$baseDir\details\screenshots\ss_$($ss.id).orig.jpg"
    $outPath = "$baseDir\details\screenshots\ss_$($ss.id).webp"
    if (-not (Test-Path $outPath)) {
        Invoke-WebRequest -Uri $ss.path_full -OutFile $origPath
        ffmpeg -i $origPath -c:v libwebp -q:v 50 -y $outPath
        Remove-Item $origPath
    }
    $i++
}

$i = 0
foreach ($vid in $data.movies) {
    if ($i -ge 1) { break }
    $mediaManifest += @{
        role = "video"
        title = $vid.name
        file = "videos/vid_$($vid.id).webm"
    }
    $mediaManifest += @{
        role = "video-thumbnail"
        title = $vid.name
        file = "video_thumbnails/vid_$($vid.id).webp"
    }

    $origThumb = "$baseDir\details\video_thumbnails\vid_$($vid.id).orig.jpg"
    $outThumb = "$baseDir\details\video_thumbnails\vid_$($vid.id).webp"
    if (-not (Test-Path $outThumb)) {
        Invoke-WebRequest -Uri $vid.thumbnail -OutFile $origThumb
        ffmpeg -i $origThumb -c:v libwebp -q:v 50 -y $outThumb
        Remove-Item $origThumb
    }

    $outVid = "$baseDir\details\videos\vid_$($vid.id).webm"
    if (-not (Test-Path $outVid)) {
        $vidUrl = $vid.hls_h264
        # Download and compress HLS stream via FFmpeg
        ffmpeg -i $vidUrl -c:v libvpx-vp9 -b:v 0 -crf 35 -vf "scale=-2:720,fps=24" -c:a libopus -b:a 64k -y $outVid
    }

    $i++
}

$mediaManifest | ConvertTo-Json -Depth 10 | Out-File -FilePath "$metadataDir\media-manifest.json" -Encoding UTF8

Write-Host "Done."
