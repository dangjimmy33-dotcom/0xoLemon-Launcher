// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include <cstdint>
#include <string>

namespace Injection {

    struct Settings {
        bool enabled = false;
        std::string libraryX64;
        std::string libraryX86;
    };

    // Try to inject the configured library into the given process.
    // Safe to call multiple times per process; only the first call injects.
    void Apply(uint32_t pid);

    // Load settings from config
    Settings LoadSettings();

}
