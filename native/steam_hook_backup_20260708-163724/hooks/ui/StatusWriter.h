// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

// Status writer for SteaMidra. Every hook installer reports a hit or miss as
// it runs; once init is done the data gets flushed to <Steam>\lumacore\status.json
// so the GUI can poll the file and surface a banner when the running Steam
// build's TOML hasn't been uploaded yet.
//
// Schema:
//   {
//     "build_id": "1779918128",
//     "steamclient_sha": "<64 hex>",
//     "steamui_sha":     "<64 hex>",
//     "steamclient_toml_found": true,
//     "steamui_toml_found":     true,
//     "hooks_installed": ["LoadPackage", "CheckAppOwnership", ...],
//     "hooks_missed":    ["FindOrCreateKey", ...],
//     "ts": 1735312453
//   }
//
// All public functions are thread-safe — the IPC and capture installers run
// from worker threads inside the same init pass.

#include <cstdint>
#include <string>

namespace StatusWriter {

    // One-shot init from entry.cpp after build id detection. Stores build id
    // into the in-memory status so Flush emits it.
    void Init(const std::string& buildId);

    // Records the SHA + per-subdir TOML availability after the fetcher's
    // synchronous cache prime. Called from entry.cpp once for steamclient
    // and once for steamui. Pass an empty sha when the lookup never ran
    // (e.g. steamui hasn't loaded yet).
    void RecordTomlState(const char* subdir, const std::string& sha, bool tomlFound);

    // Hit/miss recorders called from the hook macros. funcName is the same
    // bare identifier that the macro stringifies (e.g. "LoadPackage").
    void RecordHit(const char* funcName);
    void RecordMiss(const char* funcName);

    // Writes the accumulated status to <Steam>\lumacore\status.json. Safe to
    // call multiple times — the writer rebuilds the JSON each call from the
    // current in-memory state, so SteaMidra always sees the latest hit/miss
    // counts. Called from entry.cpp at end of init and again on detach.
    void Flush();
}
