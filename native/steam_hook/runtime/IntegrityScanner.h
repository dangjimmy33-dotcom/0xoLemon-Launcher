// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include <cstdint>
#include <string>

namespace ProtectionScan {

    enum class Method { None, LegacySectionString, OepPattern, ProtectedBlobSection };
    const char* ToString(Method m);

    struct Report {
        uint32_t pid = 0;
        bool denuvoDetected = false;
        Method method = Method::None;
        std::string modulePath;
        std::string sectionName;
        uint32_t moduleSize = 0;
        uint32_t entryPointRva = 0;
        uint32_t matchRva = 0;
        size_t matchRawOffset = 0;
        double elapsedMs = 0.0;
        size_t scannedModules = 0;
    };

    Report Scan(uint32_t pid);

}
