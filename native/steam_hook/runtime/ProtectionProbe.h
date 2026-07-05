// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include "Steam/Types.h"

#include <string>

namespace ProtectionProbe {
    struct ScanResult {
        bool valid = false;
        bool detected = false;
        std::string method;
    };

    ScanResult ScanOnce(uint32 pid, uint64 creation, AppId_t appId, const std::string& imagePath);
}
