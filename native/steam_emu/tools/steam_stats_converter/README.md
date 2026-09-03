# Steam Stats Converter

A Python 3 tool that converts achievements and stats between **Steam's binary
`UserGameStats` format** and the **Gold Steam Emulator (GSE) JSON/binary format**.

Requires Python 3.10 or newer (standard library only — no extra packages needed).

---

## Background

### Steam's binary format  (`appcache/stats/`)

Steam caches two files per game in `<Steam root>/appcache/stats/`:

| File | Purpose |
|------|---------|
| `UserGameStatsSchema_<appid>.bin` | Achievement & stat **definitions** — names, types, bit-positions |
| `UserGameStats_<steamid>_<appid>.bin` | Per-user **earned data** — which achievements are unlocked, timestamps, stat values |

Both files use Valve's **binary KeyValues (VDF)** format.  
Achievement groups store a 32-bit bitmask (`data`) plus an `AchievementTimes` sub-dict
(`bit_index -> unix_timestamp`).  
Stat groups store the raw int32/float32 value in `data`.

### GSE format  (`steam_settings/` + save folder)

| File | Purpose |
|------|---------|
| `steam_settings/achievements.json` | Achievement definitions (name, displayName, description, icons) |
| `steam_settings/stats.json` | Stat definitions (name, type, default) |
| `<save>/<appid>/achievements.json` | User earned data: `{"ACH_NAME": {"earned": true, "earned_time": 1234567890}}` |
| `<save>/<appid>/stats/<stat_name>` | Raw 4-byte int32 (INT) or float32 (FLOAT/AVGRATE) per stat |

---

## Quick start

**Windows** — double-click or call from a terminal:
```bat
run.bat --appid 1234 --st2gse
run.bat --appid 1234 --gse2st
```

**Linux / macOS**:
```bash
./run.sh --appid 1234 --st2gse
./run.sh --appid 1234 --gse2st
```

Both scripts are thin wrappers that forward all arguments to
`steam_stats_converter.py`. Run with `--help` for the full option list.

---

## Usage

### 1 — Inspect your progress

```
python steam_stats_converter.py --appid 1234 --info
```

Lists every achievement (earned/total) and every stat value for appid 1234,
read directly from your Steam installation.

---

### 2 — Steam → GSE  (`--st2gse`)

Reads the Steam Schema + UserGameStats files and writes GSE-compatible save files.

```bash
# Fully automatic (auto-detects your Steam installation and SteamID):
python steam_stats_converter.py --appid 1234 --st2gse --out-dir ./gse_save

# Explicit SteamID:
python steam_stats_converter.py --appid 1234 --st2gse --steamid 76561198012345678

# Custom output directory:
python steam_stats_converter.py --appid 1234 --st2gse --out-dir "D:\saves\1234"
```

**Output:**
```
gse_save/1234/
  achievements.json     <- GSE user achievements (earned + timestamps + _group/_bit)
  stats.json            <- GSE user stats (values + _group metadata)
```

Place these files into:
```
<GSE save folder>/<appid>/
```
The default GSE save folder on Windows is:
```
%APPDATA%\GSE Saves\<appid>\
```

---

### 3 — GSE → Steam  (`--gse2st`)

Reads GSE save files and writes a Steam `UserGameStats_<steamid3>_<appid>.bin`.

```bash
# Automatic SteamID + auto-detect GSE save dir:
python steam_stats_converter.py --appid 1234 --gse2st

# Explicit GSE save directory:
python steam_stats_converter.py --appid 1234 --gse2st --gse-dir "D:\saves\1234"

# Explicit SteamID and output path:
python steam_stats_converter.py --appid 1234 --gse2st \
    --steamid 76561198012345678 \
    --out UserGameStats_12345678_1234.bin
```

**Output:** A `.bin` file ready to be placed in:
```
<Steam root>/appcache/stats/UserGameStats_<steamid3>_<appid>.bin
```

> **Note:** The output carries forward the original CRC from the Steam bin if
> one is present; otherwise CRC is set to 0.  Steam recomputes and overwrites
> it on the next sync regardless.  All achievement and stat data is fully
> preserved, and all groups are marked as pending upload so Steam re-syncs
> everything on the next launch.

---

## Common options

| Flag | Description |
|------|-------------|
| `--appid` | Steam App ID (required) |
| `--st2gse` | Convert Steam binary → GSE save files |
| `--gse2st` | Convert GSE save files → Steam binary |
| `--info` | Show schema + your current progress/stats |
| `--steamid` | 64-bit SteamID or `auto` (default) |
| `--out-dir` | Output directory for `--st2gse` (default: `gse_save/<appid>`) |
| `--gse-dir` | GSE save directory for `--gse2st` (default: auto-detect) |
| `--out` | Output `.bin` path for `--gse2st` |
| `--lang` | Language for achievement names in `--info` (default: `english`) |
| `-v` / `--verbose` | Print every achievement/stat as it is processed |

---

## File locations

### Windows

```
Steam root:  C:\Program Files (x86)\Steam
Schema:      <Steam root>\appcache\stats\UserGameStatsSchema_<appid>.bin
User data:   <Steam root>\appcache\stats\UserGameStats_<steamid3>_<appid>.bin
```

> The filename uses the **steamid3** form (not the 64-bit steamid64).  
> steamid64 = 0x110000100000000 + steamid3

### Linux / macOS

```
~/.steam/steam/appcache/stats/
~/.local/share/Steam/appcache/stats/
```

---

## Round-trip accuracy

Tested on multiple games — every achievement and stat value in the converted
output matches the original Steam file exactly.  Fields that intentionally
differ (`crc`, `PendingChanges`, `pendingbits`, `state`) are sync-control
metadata that Steam overwrites on the next launch, so they do not affect data
integrity.
