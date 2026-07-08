// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

// Startup diagnostics collector. Captures the Steam build ID and
// steamclient SHA256 at init time so a diagnostic popup can surface
// the state when IPC specs fail to load.
//
// The popup is gated by Settings::diagnosticPopupEnabled (default
// false). When enabled, ReportMissing() spawns a detached thread
// that shows a MessageBoxA with the captured data — useful for
// users sharing diagnostics when a Steam update breaks dispatch.
namespace BootDiag {

    // Capture the current build ID and compute the steamclient SHA.
    // Called from InitThread after LoadDiversion().
    void Capture();

    // Show a non-blocking MessageBoxA popup on a detached thread.
    // Content includes the build ID and SHA captured above.
    // This is a read-only diagnostic — never modifies user files.
    void ReportMissing();

} // namespace BootDiag
