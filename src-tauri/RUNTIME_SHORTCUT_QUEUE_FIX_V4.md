# 0xoLemon Runtime, Shortcut, Queue and Update Fix v4

## Fixed issues

### 1. Desktop shortcut was not created reliably

The old implementation wrote an Internet Shortcut (`.url`) that depended on the custom `0xolemon://` protocol being registered correctly. The new implementation creates a native Windows Shell Link (`.lnk`) through `WScript.Shell` and points it directly to the installed launcher executable.

The shortcut passes:

- `--launch-game`
- `--install-path`
- `--launch-executable` for single-executable games

For multi-executable games, `--launch-executable` is intentionally omitted. Opening the shortcut then routes back to the normal launcher option picker instead of silently choosing an executable.

Legacy `.url` shortcuts and old bootstrap executables are removed when a shortcut is refreshed.

### 2. Play failed for games with multiple executable options

The launcher previously forced every normalized launch process to run as administrator. That changed process ownership and prevented normal child-process tracking. Generated launch options also defaulted to administrator mode.

The fix:

- no longer forces `runAsAdmin = true`;
- generates normal launch options with `runAsAdmin = false`;
- preserves administrator mode only when a game configuration explicitly requests it;
- tracks normal processes through `std::process::Child`;
- tracks elevated processes through the PID returned by PowerShell `Start-Process -PassThru`;
- makes multi-executable desktop shortcuts reopen the launch picker.

### 3. Play button stayed on Running after the game closed

The frontend previously marked a game as running immediately after any successful launch command, even when the backend did not own a trackable process.

The backend is now authoritative:

1. launch the selected main process;
2. obtain its PID;
3. call `platform::begin_game_session`;
4. emit `launcher://game-started`;
5. wait for the normal child or elevated PID;
6. call `platform::end_game_session`;
7. emit `launcher://game-exited`.

The frontend only changes to Running after the backend start event. It returns to Play after the exit event and refreshes persisted runtime state.

### 4. Game Turbo dialog looked transparent and generated

The dialog was rebuilt as a restrained launcher-native system panel:

- opaque `#0b1014` surface;
- graphite borders;
- one muted yellow accent;
- no gradients;
- no glass card transparency;
- compact information hierarchy;
- reduced-motion support.

### 5. Completed downloads remained visible and false updates appeared

Several independent causes were corrected:

- committed job journals are cleared after the brief success state;
- a committed journal left behind by closing the launcher is cleared on the next startup;
- the Downloads empty state says the game is installed and shows no install action;
- network recovery only resumes a job that was interrupted by an actual offline event during the current launcher session;
- game and patch auto-update schedulers share the same explicit policy gate;
- automatic game updates now default to Manual;
- existing schema-v1 profiles are migrated once from the old silent automatic default to Manual;
- version labels are compared canonically, so `1.01.1.0.3 (Build 22800422) - Uploaded 2026-07-15` equals `1.01.1.0.3`.

## Update-engine compatibility

The byte-range, local chunk reuse, delta and encrypted depot modules were not modified:

- `job/direct.rs`
- `job/sequential.rs`
- `manifest.rs`
- `depot_crypto.rs`

Their SHA-256 hashes match the uploaded backend source exactly.

## Main files changed

### Backend

- `lib.rs`
- `job.rs`
- `job/dependencies.rs`
- `launch.rs`
- `platform.rs`

### Frontend

- `App.tsx`
- `components/downloads.tsx`
- `components/ActiveView.tsx`
- `components/GameTurboModal.tsx`
- `components/GameTurboModal.css`
- `lib/version.ts`

## Verification performed

- frontend runtime and queue regression tests: passed;
- logical transfer progress tests: passed;
- patch progress contract tests: passed;
- UI contract tests: passed;
- backend runtime contract tests: passed;
- TypeScript/TSX syntax transpilation for modified files: passed;
- Rust delimiter/string/comment structural scan: passed;
- critical update-engine file hash comparison: unchanged;
- output ZIP CRC integrity tests: passed.

## Build limitation

The uploaded source folders do not contain `Cargo.toml`, `package.json`, lockfiles or the complete Tauri project. The environment also does not contain a Rust toolchain. Therefore a full `cargo check`, `cargo test`, Vite build and Tauri package build could not be run here.

After copying these folders into the complete project, run:

```powershell
cargo fmt --check
cargo test
cargo check --release
npm ci
npm run build
npm run tauri build
```
