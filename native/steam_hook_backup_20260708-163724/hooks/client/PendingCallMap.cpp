// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "PendingCallMap.h"

#include <mutex>
#include <array>
#include <cstring>

namespace PendingCallMap {

namespace {
    // tiny fixed slab — encrypted ticket calls are rare, never more than a handful
    constexpr size_t kMaxCalls = 16;

    struct CallSlot {
        SteamAPICall_t handle = k_uAPICallInvalid;
        AppId_t       app    = k_uAppIdInvalid;
    };

    std::mutex g_callLock;
    CallSlot   g_slots[kMaxCalls]{};
}

void RecordEncryptedTicket(SteamAPICall_t call, AppId_t appID)
{
    if (call == k_uAPICallInvalid || appID == k_uAppIdInvalid) return;

    std::lock_guard<std::mutex> hold(g_callLock);
    for (auto& sl : g_slots) {
        if (sl.handle != k_uAPICallInvalid) continue;
        sl.handle = call;
        sl.app    = appID;
        break;
    }
}

std::optional<AppId_t> TakeEncryptedTicket(SteamAPICall_t call)
{
    std::lock_guard<std::mutex> hold(g_callLock);
    for (auto& sl : g_slots) {
        if (sl.handle != call) continue;
        AppId_t out = sl.app;
        sl = CallSlot{};
        return out;
    }
    return std::nullopt;
}

}
