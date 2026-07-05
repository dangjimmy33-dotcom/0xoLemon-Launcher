// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "hooks/client/AsyncTicketMap.h"

#include <mutex>
#include <unordered_map>

namespace {
    std::mutex g_asyncLock;
    std::unordered_map<SteamAPICall_t, AppId_t> g_eticketRequests;
}

namespace AsyncTicketMap {
    void Remember(SteamAPICall_t call, AppId_t appId) {
        if (call == k_uAPICallInvalid || appId == 0 || appId == k_uAppIdInvalid)
            return;

        std::scoped_lock lock(g_asyncLock);
        g_eticketRequests[call] = appId;
    }

    std::optional<AppId_t> Claim(SteamAPICall_t call) {
        std::scoped_lock lock(g_asyncLock);
        const auto it = g_eticketRequests.find(call);
        if (it == g_eticketRequests.end())
            return std::nullopt;

        AppId_t appId = it->second;
        g_eticketRequests.erase(it);
        return appId;
    }

    void Forget(SteamAPICall_t call) {
        std::scoped_lock lock(g_asyncLock);
        g_eticketRequests.erase(call);
    }

    void Reset() {
        std::scoped_lock lock(g_asyncLock);
        g_eticketRequests.clear();
    }
}
