# Per-game launch options

## File to edit for each game

Create this file in the source asset folder of that game:

```text
E:\007Launcher\src\assets\<game-folder>\launch.json
```

Example:

```text
E:\007Launcher\src\assets\stellar-blade\launch.json
```

Then rebuild the asset pack. `launch.json` is embedded into that game's `core.0xo`, so the final launcher does not need the raw JSON beside the executable.

For quick testing without rebuilding an asset pack, place an override file in the installed game root:

```text
<Game install folder>\0xo-launch.json
```

The install-folder override has priority over the configuration embedded in `core.0xo`.

## Picker behavior

- `pickerMode: "auto"`: show the picker only when more than one option exists.
- `pickerMode: "always"`: always show the picker.
- `pickerMode: "never"`: launch the default option directly.
- `defaultOptionId`: option selected by default.

## Process behavior

Processes in an option run in array order.

- `path`: path relative to the installed game folder. Parent traversal (`..`) is rejected.
- `args`: command-line arguments.
- `workingDirectory`: relative working directory. Leave empty to use the process file's folder.
- `role`: `main` or `helper`. The main process is used for the shortcut/icon/report.
- `runAsAdmin`: launch through Windows UAC.
- `hidden`: suppress the console window. If omitted for `.bat`/`.cmd`, hidden mode is used automatically.
- `waitForExit`: wait for this process to finish before continuing to the next process.
- `delayBeforeMs`: delay before this process.
- `delayAfterMs`: delay after starting/completing this process.
- `optional`: skip the process if its file is missing instead of disabling the whole option.
- `environment`: per-process environment variables.

Supported placeholders in arguments, environment values, and working directories:

```text
{installDir}
${INSTALL_DIR}
{gameId}
${GAME_ID}
```

## Online mode with a dedicated helper

Use two processes in one option. Start the helper first with `waitForExit: false`, optionally add `delayAfterMs`, then start the main game executable. A batch file can be hidden with `hidden: true`, so no command window appears.

See `launch.example.json` for a complete DX11, DX12, offline, and online example.

## Backward compatibility

Games without `launch.json` keep the previous behavior: the launcher starts the existing `launchExecutable` directly. Older `.0xo` packs still deserialize because the new launch configuration field is optional/defaulted.
