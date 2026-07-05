// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include "Steam/Types.h"

#include <optional>

namespace AsyncTicketMap {
    void Remember(SteamAPICall_t call, AppId_t appId);
    std::optional<AppId_t> Claim(SteamAPICall_t call);
    void Forget(SteamAPICall_t call);
    void Reset();
}
