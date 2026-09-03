# Third-party notices

GSE / UC Setup orchestrates third-party components but does not claim ownership of them.

- **GSE / gse_fork** — alex47exe / upstream contributors. The embedded GSE baseline is taken from the supplied official Windows release package. Preserve upstream license/credits files shipped under the embedded resource tree.
- **gse_fork_tools** — alex47exe / upstream contributors. Downloaded as a portable component when the official generator is requested.
- **Steamless** — atom0s. V1.8 can use `Steamless.CLI.exe` and its plugins. Preserve the upstream release licensing/attribution when redistributing builds.
- **UC Online2** — UnionCrax-Team. V1.8 can download/use the project's release DLLs and plugins as a separate engine.
- **RUNE SteamStub component** — obtained from `Mush-iii/rune-emu` release assets when explicitly requested or during an optional build-time baseline refresh.
- **migrate_gse** — GSE/gbe_fork migration utility; bundled from the supplied package.
- **7-Zip standalone command line** — native `7za.exe` is retained with its bundled license notice.

Each upstream project remains governed by its own license and notices. V1.8 keeps update components isolated under `resources/updates/<component>` so they can be replaced independently.
