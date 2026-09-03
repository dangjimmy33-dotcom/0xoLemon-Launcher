# GSE connectivity preset fix

- Replaced ambiguous **Offline** preset with three explicit modes:
  - **Single-player (recommended):** `disable_networking=1`, `offline=0`
  - **Strict offline:** `disable_networking=1`, `offline=1`
  - **LAN:** `disable_networking=0`, `offline=0`
- Existing `network_mode=offline` values in `config.ini` automatically migrate to `singleplayer`.
- This avoids games interpreting GSE as a disconnected Steam session just because the user wanted single-player networking disabled.
