// Embedded resources: the native cloud_redirect.dll and per-Steam-build payload
// caches, baked into the binary at compile time.

use crate::{crypto, file_util};
use std::path::{Path, PathBuf};

// The native DLL deployed next to steam.exe. Built by `cmake` into
// build/Release/cloud_redirect.dll before this crate is compiled.
const CLOUD_REDIRECT_DLL: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/embedded/cloud_redirect.dll"));

// Per-build SteamTools payload caches. Each entry: (steam_build, bytes).
// The files live in cli-rust/embedded/payloads/<build>/payload (copied from
// ui/Resources/payloads by the build script).
const PAYLOADS: &[(i64, &[u8])] = &[
    (
        1782866176,
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/embedded/payloads/1782866176/payload")),
    ),
];

pub fn dll_available() -> bool {
    !CLOUD_REDIRECT_DLL.is_empty()
}

/// Deploy the embedded cloud_redirect.dll to `dest`. Returns Some(error) on failure.
pub fn deploy_dll(dest: &Path) -> Option<String> {
    if CLOUD_REDIRECT_DLL.is_empty() {
        return Some("cloud_redirect.dll is not embedded in this build.".to_string());
    }
    match file_util::atomic_write_all_bytes(dest, CLOUD_REDIRECT_DLL) {
        Ok(()) => None,
        Err(e) => {
            // Likely "file in use" when Steam is running.
            if e.raw_os_error() == Some(32) {
                Some("cloud_redirect.dll is in use -- close Steam first.".to_string())
            } else {
                Some(format!("Failed to deploy cloud_redirect.dll: {}", e))
            }
        }
    }
}

fn payload_for_build(steam_build: i64) -> Option<&'static [u8]> {
    PAYLOADS
        .iter()
        .find(|(b, _)| *b == steam_build)
        .map(|(_, data)| *data)
}

/// Install the embedded payload cache for `steam_build` into the expected
/// fingerprint slot, validating it afterwards. Returns the cache path on success.
pub fn install_payload(
    steam_path: &Path,
    steam_build: i64,
    mut log: impl FnMut(&str),
) -> Option<PathBuf> {
    let data = match payload_for_build(steam_build) {
        Some(d) => d,
        None => {
            log(&format!(
                "Embedded payload: not present for build {}",
                steam_build
            ));
            return None;
        }
    };

    let dst = crypto::get_expected_cache_path(steam_path);
    if let Some(dir) = dst.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    if let Err(e) = file_util::atomic_write_all_bytes(&dst, data) {
        log(&format!("Embedded payload install failed: {}", e));
        return None;
    }

    if !crypto::validate_payload_cache(&dst) {
        log(&format!(
            "Embedded payload for build {} failed validation after install",
            steam_build
        ));
        let _ = std::fs::remove_file(&dst);
        return None;
    }

    log(&format!("Embedded payload installed to {}", dst.display()));
    Some(dst)
}
