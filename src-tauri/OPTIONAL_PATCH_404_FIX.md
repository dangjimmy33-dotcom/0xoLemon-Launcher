# Optional patch manifest HTTP 404 fix

## Symptom

An ordinary game update finishes `Stream update`, then fails in `Commit transaction` with an error similar to:

`unable to load patches/<version>/manifest.json: ... HTTP 404`

This occurs for games/versions that intentionally have no patch-fix manifest.

## Root cause

`send_remote_get()` correctly returned `JobError::NotFound` for HTTP 404. However, `DepotSource::load_json()` converted failures from every configured mirror into one generic `JobError::Depot` string. Consequently, `load_patch_manifest()` could not recognize a normal missing optional object and return `Ok(None)`.

`obsolete_update_paths()` calls `load_patch_manifest()` during the transaction phase so it can retain files owned by a target-version patch. The generic depot error therefore aborted the main update before the install marker was committed.

## Fix

- Preserve `JobError::NotFound` when every configured remote candidate returns HTTP 404.
- Treat a missing object in a local-only depot as `NotFound` as well.
- Cache missing JSON paths inside the job's `DepotSource`, avoiding a second round of mirror requests later in the same update.
- Keep mixed failures strict: for example, `404 + 500`, malformed JSON, authorization failures, or other content-service errors are not silently treated as "no patch".
- Add a unit regression test for the all-404 classification.

## Result

For `patches/v2.16.2/manifest.json` when no patch exists:

`HTTP 404 on all mirrors -> JobError::NotFound -> load_patch_manifest() returns Ok(None) -> update continues -> Patch fix is marked No patch required`

## Unchanged systems

The byte/chunk/delta update pipeline was not modified. `job/direct.rs`, `job/sequential.rs`, and `manifest.rs` are byte-for-byte unchanged from the previous optimized package.

## Recovery from the currently failed job

Build the corrected backend, restart the launcher, and press **Resume**. Keep the existing install, journal, download directory, and transaction files. Because the failure occurred before the install marker commit, the transaction layer may have rolled installed files back for safety; the launcher will re-evaluate valid local/staged/cache data during resume. Depending on what remained durable, some assembly or network transfer may be repeated.

## Verification performed in this environment

- Regression source check: pass.
- Rust delimiter/lexical balance check: pass.
- Patch whitespace check: pass.
- Confirmed only `src_backend/job.rs` changed.
- Confirmed `job/direct.rs`, `job/sequential.rs`, and `manifest.rs` hashes are unchanged.
- ZIP integrity checks: pass.

A full Rust build/test could not be run because the supplied source archives contain no `Cargo.toml` and this environment has no Rust toolchain.
