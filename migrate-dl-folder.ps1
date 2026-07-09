# Migration script: Rename "downloading" folders to "dl"
# Run this ONCE after updating to the new version

$ErrorActionPreference = "Continue"

Write-Host "=== 0xoLemon Download Folder Migration ===" -ForegroundColor Cyan
Write-Host "This script will rename 'downloading' folders to 'dl' to reduce path length" -ForegroundColor Yellow
Write-Host ""

# Find all library roots
$libraryRoots = @()

# Check default location
if (Test-Path "E:\0xoLemon store") {
    $libraryRoots += "E:\0xoLemon store"
}

# Check other drives
foreach ($drive in (Get-PSDrive -PSProvider FileSystem | Where-Object { $_.Root -match '^[A-Z]:\\$' })) {
    $possiblePath = Join-Path $drive.Root "0xoLemon store"
    if ((Test-Path $possiblePath) -and ($possiblePath -notin $libraryRoots)) {
        $libraryRoots += $possiblePath
    }
}

if ($libraryRoots.Count -eq 0) {
    Write-Host "No 0xoLemon libraries found. Nothing to migrate." -ForegroundColor Green
    exit 0
}

Write-Host "Found $($libraryRoots.Count) library location(s):" -ForegroundColor Cyan
foreach ($root in $libraryRoots) {
    Write-Host "  - $root" -ForegroundColor White
}
Write-Host ""

$totalRenamed = 0

foreach ($root in $libraryRoots) {
    $oldPath = Join-Path $root "downloading"
    $newPath = Join-Path $root "dl"
    
    if (Test-Path $oldPath) {
        Write-Host "Processing: $root" -ForegroundColor Cyan
        
        # Check if new path already exists
        if (Test-Path $newPath) {
            Write-Host "  WARNING: '$newPath' already exists!" -ForegroundColor Yellow
            Write-Host "  Merging contents..." -ForegroundColor Yellow
            
            # Move contents instead of renaming folder
            Get-ChildItem $oldPath | ForEach-Object {
                $destPath = Join-Path $newPath $_.Name
                if (Test-Path $destPath) {
                    Write-Host "    Skipping existing: $($_.Name)" -ForegroundColor Gray
                } else {
                    Move-Item $_.FullName $destPath -Force
                    Write-Host "    Moved: $($_.Name)" -ForegroundColor Green
                }
            }
            
            # Remove old folder if empty
            if ((Get-ChildItem $oldPath | Measure-Object).Count -eq 0) {
                Remove-Item $oldPath -Force
                Write-Host "  Removed empty folder: $oldPath" -ForegroundColor Green
            }
        } else {
            # Simple rename
            try {
                Rename-Item $oldPath $newPath -Force
                Write-Host "  ✅ Renamed: downloading → dl" -ForegroundColor Green
                $totalRenamed++
            } catch {
                Write-Host "  ❌ Failed to rename: $_" -ForegroundColor Red
            }
        }
    } else {
        Write-Host "No 'downloading' folder found in: $root" -ForegroundColor Gray
    }
    Write-Host ""
}

Write-Host "=== Migration Complete ===" -ForegroundColor Green
Write-Host "Total folders renamed: $totalRenamed" -ForegroundColor White
Write-Host ""
Write-Host "Path length savings: ~7 characters per file path" -ForegroundColor Cyan
Write-Host "This should reduce IO Error 2 occurrences!" -ForegroundColor Cyan
Write-Host ""
Write-Host "Press any key to exit..."
$null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
