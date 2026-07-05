// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include "core/entry.h"

#include <optional>

namespace PendingCallMap {

    void RecordEncryptedTicket(SteamAPICall_t call, AppId_t appID);
    std::optional<AppId_t> TakeEncryptedTicket(SteamAPICall_t call);

}
