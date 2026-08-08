# CloudRedirect 2.6.3 Windows Build Checklist

## Required software

- Visual Studio 2022 or Build Tools 2022
- Desktop development with C++ workload
- .NET 8 workload/SDK required by the upstream Windows project
- CMake 3.20 or newer
- Rust/Cargo toolchain used by the launcher
- Node.js and existing launcher dependencies

## Optional Cloud760 dependency

The user-supplied source archive does not contain the 32-bit `steam_api.dll` embedded by upstream release packaging.

To enable Cloud760, set the environment variable to a trusted copy from the official CloudRedirect release asset before building:

```powershell
$env:CLOUDREDIRECT_STEAM_API_DLL = 'C:\Path\To\Official\steam_api.dll'
```

Without it, the main CloudRedirect engine still builds and works; only Cloud760 is disabled with a clear diagnostic.

## Build

Run from the launcher root:

```powershell
cd E:\007Launcher

powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\src-tauri\build-cloudredirect.ps1 `
  -Configuration Release

npm run build

cd .\src-tauri
cargo check --lib
cd ..

npm run tauri build
```

The Tauri `beforeBuildCommand` already invokes `build-cloudredirect.ps1`, so the explicit native build is mainly useful for diagnosing CMake/MSVC errors before the full build.

## Expected native outputs

```text
src-tauri\resources\cloud_redirect\engine\2.6.3\
├── cloud_redirect.dll
├── cloud_redirect_cli.exe
├── cloud760_tool.exe
├── engine.json
└── steam_api.dll        # optional, required only for Cloud760
```

## Runtime smoke test

1. Open Cloud Saves → STFixer / CloudRedirect.
2. Verify engine status shows 2.6.3 and the source commit.
3. Close Steam.
4. Select the intended mode and run all required patches.
5. Configure one provider and test the connection.
6. Start Steam and confirm the DLL is loaded through the CloudRedirect log.
7. Scan remote apps and open one app's file inventory.
8. Close Steam before creating/restoring backups or deleting cloud data.
9. Test a provider migration with non-critical test data first.
10. Verify the source provider remains intact after migration.

## Release gate

Do not publicly ship the upstream source/binaries until redistribution permission has been obtained from the CloudRedirect copyright holder or an applicable license has been published.
