# 0xoLemon backend split + download path fix

This package is the edited `src-tauri/src` tree. It splits the large `job.rs` into smaller modules:

- `job/dependencies.rs` - VC redist check/download, shortcut creation, elevated launch helpers.
- `job/paths.rs` - install/default paths, staged chunk paths, and the fixed downloading folder mapper.
- `job/progress.rs` - progress/percentage/byte formatting helpers.
- `job.rs` - main Tauri job API and orchestration flow.

## Main bug fixed

Install path:

```txt
E:\0xoLemon store\common\Geometry Dash
```

now stages downloads at:

```txt
E:\0xoLemon store\downloading\Geometry Dash
```

instead of:

```txt
E:\0xoLemon store\common\Geometry Dash\.0xolemon\downloading
```

## Apply

Copy/overwrite the contents of this zip into:

```txt
E:\007Launcher\src-tauri\src
```

Then run:

```powershell
cd E:\007Launcher
npm run build
cd src-tauri
cargo check --release
cd ..
npm run tauri dev
```

If Cargo reports an import/visibility error, send the full `cargo check --release` log.
