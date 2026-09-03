# GSE / UC Setup V1.8.2

## Fix: official generator no longer appears frozen forever

- Setup now uses the official `_DEFAULT/1` complete GSE preset with `-skip_ach` because Steam Web API achievement/stat enrichment is already fetched before generator launch.
- This avoids running the slow anonymous achievement-owner scan twice while retaining the generator's other game data: depots, branches, controller config, inventory, languages, DLC/config output, etc.
- Generator stdout is read on a background reader thread.
- The UI emits a heartbeat every 10 seconds while the generator is alive.
- 120 seconds without generator output aborts cleanly instead of hanging forever.
- Overall generator runtime is capped at 600 seconds.
- Unexpected stdin prompts cannot block the windowed app.

## Fix: resources are actually portable beside the EXE

After a Windows build, `dist` now contains:

```text
GSEAutoSetup.exe
resources/
  embedded/
    gse/
    gse_tools/
    uc_online/
    steamless/
    rune_steamstub/
    migrate_gse/
  7zip/
```

Runtime priority is now:

1. `resources/updates/<component>` beside the EXE
2. `resources/embedded/<component>` beside the EXE
3. embedded one-file PyInstaller fallback

A valid local `gse_fork_tools` package is used immediately during Setup; Setup does not re-download it just to compare release versions. Component update checks belong to the resource updater flow.

A stale/partial `resources/updates/gse_tools` folder can no longer hide a valid external/embedded generator because candidates are validated in order.
