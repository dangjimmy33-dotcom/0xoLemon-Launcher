// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

namespace KVHooks {
    // Hooks KeyValues::ReadAsBinary so the keyvalue category gets at least
    // one entry per session. Triage on KV-tree regressions needs the file
    // non-empty.
    void Install();
    void Uninstall();
}
