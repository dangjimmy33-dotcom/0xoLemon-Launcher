# HF incremental upload fix

This tool version removes the heavy `.hf_upload_stage` workflow.

Behavior:
- Builds depot locally.
- Uploads `packs/*.bin` one by one with `HfApi.upload_file`.
- Uploads metadata/catalog only after packs are uploaded.
- If **Giữ local transport packs** is unchecked, deletes each local pack immediately after its upload succeeds.
- No `.hf_upload_stage` folder is created.
- `HF_HUB_DISABLE_PROGRESS_BARS=1` by default to avoid ANSI log spam.
- `HF_XET_HIGH_PERFORMANCE=0` by default to avoid CPU/disk spikes on Windows; set it manually to `1` if you want aggressive transfer.

If a previous run crashed because `.hf_upload_stage` was huge, remove it once:

```powershell
Remove-Item "E:\007Launcher\depot\stellar-blade\.hf_upload_stage" -Recurse -Force -ErrorAction SilentlyContinue
```
