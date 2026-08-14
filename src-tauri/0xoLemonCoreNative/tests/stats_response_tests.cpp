#include "runtime/StatsResponse.h"

#include <cassert>
#include <chrono>
#include <cstdint>

int main() {
    using namespace StatsClient::Detail;

    std::uint64_t steamId = 0;
    assert(ParseSteamId64("76561198017975643", steamId));
    assert(steamId == 76561198017975643ULL);
    assert(ParseSteamId64("  76561198017975643\r\n", steamId));
    assert(!ParseSteamId64("123", steamId));
    assert(!ParseSteamId64("76561198017975643x", steamId));
    assert(!ParseSteamId64("18446744073709551616", steamId));

    const auto success = ClassifyResponse(false, 200, "76561198017975643");
    assert(success.steamId == 76561198017975643ULL);
    assert(success.ttl == kPositiveTtl);

    const auto malformed = ClassifyResponse(false, 200, "not-a-steamid");
    assert(malformed.steamId == 0);
    assert(malformed.ttl == kNegativeTtl);

    const auto forbidden = ClassifyResponse(false, 403, "76561198017975643");
    assert(forbidden.steamId == 0);
    assert(forbidden.ttl == kNegativeTtl);

    const auto timeout = ClassifyResponse(true, 0, "");
    assert(timeout.steamId == 0);
    assert(timeout.ttl == kNegativeTtl);

    const auto now = std::chrono::steady_clock::now();
    assert(IsFresh(now + std::chrono::seconds(1), now));
    assert(!IsFresh(now, now));
    assert(!CanEnqueue(true, false, 0, 128));
    assert(!CanEnqueue(false, true, 0, 128));
    assert(!CanEnqueue(false, false, 128, 128));
    assert(CanEnqueue(false, false, 127, 128));
    return 0;
}
