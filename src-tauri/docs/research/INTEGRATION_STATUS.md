# Research integration status — 2026-09-03

This source snapshot carries the two audit reports used for the current fixes.

## DepotDownloader audit

Implemented in this snapshot:

- Version-switch path seeds target manifests into `.DepotDownloader` instead of invoking the fork with `-manifestfile`.
- `-verify-all` is forced on the legacy Steam patch path and on explicit version switches.
- BuildID manifests remain cached beside the working copy for A → B → A reuse.
- Pause/resume uses stop → restart while preserving final game data and DepotDownloader state.

## GSE parity audit

Implemented in this snapshot:

- Normal official generator invocation is `-def1 -clr -anon <appid>` (no default `-skip_ach`).
- Generator-produced `achievements.json`, `stats.json`, and `supported_languages.txt` are canonical and are not overwritten by Steam Web API fallback data.
- Rust fallback can fetch multiple Steam language schemas and only writes achievements when the official generator did not supply them.
- Runtime default materialization excludes README/LICENSE/CHANGELOG documentation files.
- GSE workspace label/layout is aligned with the audit (`Setup & Emulator`, centered 1240px workspace).

The original reports are stored beside this file for traceability.
