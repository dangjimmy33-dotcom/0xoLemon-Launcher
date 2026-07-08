# 🔍 Quick Test Guide - Debug Logging

## ✅ Changes Made

1. **Added `debug_log.rs`** - File logging system
2. **All `eprintln!` in `direct.rs` replaced with `dlog!`** (69 occurrences)
3. **Added Settings UI toggle** (coming next)
4. **Logs save to**: `%APPDATA%\com.dangjimmy.oxolemon\download-debug.log`

## 📍 Log File Location

Windows: `C:\Users\<YourName>\AppData\Roaming\com.dangjimmy.oxolemon\download-debug.log`

Or quick access:
```powershell
code "$env:APPDATA\com.dangjimmy.oxolemon\download-debug.log"
```

## 🧪 How to Test

### Step 1: Build
```powershell
cd E:\007Launcher\src-tauri
npm run tauri build
```

### Step 2: Run
Logs are **automatically enabled** on startup now!

### Step 3: Start Download
- Open launcher
- Choose "meccha-chameleon v1.3.1.1"
- Click "Start download"
- Let it run to ~10-20%
- Click "Pause"

### Step 4: Check Logs
```powershell
Get-Content "$env:APPDATA\com.dangjimmy.oxolemon\download-debug.log" -Tail 100
```

Look for:
```
[PREPARE] ========================================
[CHECKPOINT] Starting checkpoint for X chunks
[PERSIST_STATE] Writing ... bytes (X completed chunks)
[PERSIST_STATE] ✅ State file saved successfully
```

### Step 5: Resume Test
- Click "Resume"
- Check logs again:
```
[PREPARE] Loaded existing state:
[PREPARE]   - Completed chunks: X
[FILTER_MISSING] Resume progress: XX.X%
```

## 🔍 What to Look For

### ✅ Good Signs:
- `[PERSIST_STATE] ✅ State file saved successfully` appears regularly
- `[PREPARE] ✅ Existing state is valid` when resuming
- `[FILTER_MISSING] Resume progress: XX.X%` matches UI progress

### ❌ Bad Signs:
- `[PREPARE] ⚠️ State mismatch detected` on resume
- `[FILTER_MISSING] Resume progress: 0.0%` after partial download
- `[CHECKPOINT] ERROR: File not found during checkpoint`

## 📝 Share Logs

If issue persists, share the log file content from last download attempt:
```powershell
Get-Content "$env:APPDATA\com.dangjimmy.oxolemon\download-debug.log" | Select-Object -Last 500 | Out-File "E:\007Launcher\debug-output.txt"
```

Then paste `debug-output.txt` content.
