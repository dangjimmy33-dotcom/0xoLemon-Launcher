// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include <cstdint>
#include "Steam/Types.h"
#include <string>
#include <string_view>
#include <optional>
#include <unordered_map>

namespace ProcessInspect {

    struct Environment {
        std::optional<AppId_t> steamOverlayGameId;
        std::optional<AppId_t> steamGameId;
        std::optional<AppId_t> steamAppId;

        AppId_t ResolveAppId() const;
        bool HasSteamAppEnvironment() const;
    };

    struct Snapshot {
        uint32_t pid = 0;
        uint64_t creationTime = 0;
        std::string imagePath;
        std::string imageName;
        bool steamClientProcess = false;
        bool likelyGameProcess = false;
        Environment env;

        AppId_t ResolveAppId() const { return env.ResolveAppId(); }
    };

    using ProcessKey = std::pair<uint32_t, uint64_t>;
    struct ProcessKeyHash {
        size_t operator()(const ProcessKey& k) const {
            return std::hash<uint32_t>()(k.first) ^ (std::hash<uint64_t>()(k.second) << 1);
        }
    };

    std::optional<uint64_t> GetProcessCreationTime(uint32_t pid);
    bool IsSteamProcessName(std::string_view name);
    Environment ReadSteamEnvironment(uint32_t pid);
    Snapshot InspectProcess(uint32_t pid);
    Snapshot GetCachedOrInspect(uint32_t pid);

    constexpr std::string_view kSteamProcessNames[] = {
        "steam.exe",
        "steamwebhelper.exe",
        "steamservice.exe",
        "steamerrorreporter.exe",
        "gameoverlayui.exe",
        "gameoverlayui64.exe",
    };

}
