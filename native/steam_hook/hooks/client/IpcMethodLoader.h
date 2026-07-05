// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include <cstdint>
#include <string>
#include <string_view>

namespace IpcLoader {

    struct MethodMeta {
        uint32_t funcHash = 0;
        uint32_t fencepost = 0;
        uint32_t argc = 0;
    };

    // Parses ipc_methods.toml alongside the steamclient dll path.
    // The caller supplies the steamclient path so we can derive the TOML path.
    // Falls back to a remote fetch from the pattern repo when no local file exists.
    bool Load(const std::string& steamclientPath);

    bool IsLoaded();

    const MethodMeta* Find(std::string_view ifaceName, std::string_view methodName);

    // FNV-1a hash helpers used by the dispatch table
    uint32_t HashInterfaceName(std::string_view name);
    uint32_t HashMethodName(std::string_view name);

}
