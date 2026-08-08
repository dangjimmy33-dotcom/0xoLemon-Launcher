# Native SFF core port

The Python `sff/core` layer from the supplied SFF tree has been reduced to native services that are compiled into `0xoCore.dll`. No CPython runtime is embedded.

## Mapped modules

| Python module | Native implementation | Notes |
|---|---|---|
| `sff/core/cache.py` | `source/sffcore/CoreCache.*` | Thread-safe TTL cache with atomic native persistence. |
| `sff/core/integrity.py` | `source/sffcore/CoreIntegrity.*` | Manifest magic/size checks and Windows CNG SHA-256. |
| `sff/core/processes.py` | `source/sffcore/CoreProcess.*` | Win32 process enumeration, elevation check, Steam launch/kill helpers. UI prompts intentionally removed. |
| `sff/core/secret_store.py` | `source/sffcore/CoreSecretStore.*` | Windows DPAPI replaces Python keyring/PyNaCl runtime dependencies. |
| `sff/core/utils.py` | `source/sffcore/CorePaths.*`, `KeyValues.*` | Native module/data path handling and structured traversal. |
| `sff/core/storage/vdf.py` | `source/sffcore/KeyValues.*`, `CoreAcf.*` | Native Steam KeyValues parser/writer, library discovery and app registration helper. |
| `sff/core/storage/acf.py` | `source/sffcore/CoreAcf.*` | Native ACF parsing and app-state helpers. |
| `sff/core/storage/ini_config.py` | `source/sffcore/CoreIni.*` | In-place INI option conversion while retaining surrounding text. |
| `sff/core/storage/settings.py` | `source/sffcore/CoreSettings.*` | Native primitive settings persistence (`0xo_settings.toml`). |
| `sff/core/storage/named_ids.py` | `source/sffcore/CoreNamedIds.*` | Local ID/name registry and saved-Lua scan; no network lookup in core-only mode. |
| `sff/core/structs.py` | `source/sffcore/CoreTypes.h` | Only types needed by the native core are retained; menu/UI/provider-only enums are deliberately omitted. |
| `sff/core/__init__.py` | `source/sffcore/NativeCore.*` | Native bootstrap/shutdown. |

## Intentionally not carried into the DLL

- `sff/core/strings.py` contains application/update/provider metadata, not runtime-core behavior.
- `sff/core/storage/yaml.py` is only consumed by the separate SLS/app-injector path in the supplied tree; that path is outside the requested core-only DLL package.
- GUI confirmation logic from `processes.py` is excluded because this package has no frontend.

## Link model

CMake builds the native port as an **OBJECT library** (`OxoSffNativeCore`) and injects its object files directly into `OxoCore`. This creates no additional runtime library. The final install target remains exactly four DLLs.
