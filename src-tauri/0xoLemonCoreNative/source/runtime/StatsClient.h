// _0xoLemonCore - asynchronous OpenSteamTool stats lookup.

#ifndef OXOLEMONCORE_STATS_CLIENT_H
#define OXOLEMONCORE_STATS_CLIENT_H

#include "Steam/Types.h"
#include <cstdint>

namespace StatsClient {
    void Schedule(AppId_t appId);

    // Cache-only lookup. Safe for Steam packet paths; never performs I/O.
    bool TryGet(AppId_t appId, std::uint64_t& steamId);

    void Forget(AppId_t appId);
}

#endif
