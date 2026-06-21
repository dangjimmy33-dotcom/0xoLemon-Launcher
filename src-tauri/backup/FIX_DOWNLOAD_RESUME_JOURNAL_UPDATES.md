# Download/resume/journal/update-list fix

Applied to the uploaded backend baseline `src-tauri(15).zip`.

## Backend changes

- Each pack byte-range is streamed into a persistent `chunks/_ranges/*.part` file.
- Resume starts at the existing partial-file length and sends a new HTTP `Range` request for only the remaining bytes.
- A resumed response must be `206 Partial Content` and its `Content-Range` must start at the requested byte offset.
- Partial bytes are flushed/synced every 4 MiB and immediately when paused, so pause -> close launcher -> reopen -> resume keeps downloaded data.
- Progress/journal events are throttled to roughly four updates per second instead of one disk rewrite per 256 KiB.
- Existing partial byte counts are restored into install, update, and repair progress.
- `current-job.json` is written through a complete temporary file and then renamed, reducing truncated JSON after interruption.
- Malformed journals are quarantined instead of trapping the launcher in Downloads.
- Cancel/uninstall retry journal deletion and staging-directory deletion on Windows.
- Canceled worker threads cannot rewrite the job as failed after cleanup.
- Late cleanup is guarded by job ID so it cannot delete a newer job.

## Validation

- ZIP/source integrity checks: passed.
- Rust delimiter/static inspection: passed.
- `cargo check` was not run because the validation environment did not contain a Rust toolchain.
