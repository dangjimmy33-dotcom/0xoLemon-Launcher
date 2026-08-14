// _0xoLemonCore - pure validation helpers for the OpenSteamTool stats client.

#ifndef OXOLEMONCORE_STATS_RESPONSE_H
#define OXOLEMONCORE_STATS_RESPONSE_H

#include <algorithm>
#include <charconv>
#include <chrono>
#include <cstddef>
#include <cstdint>
#include <string_view>

namespace StatsClient::Detail {
    constexpr auto kPositiveTtl = std::chrono::hours(24);
    constexpr auto kNegativeTtl = std::chrono::minutes(30);

    struct ParsedResponse {
        std::uint64_t steamId = 0;
        std::chrono::steady_clock::duration ttl = kNegativeTtl;
    };

    inline bool ParseSteamId64(std::string_view body, std::uint64_t& steamId) {
        const auto first = body.find_first_not_of(" \t\r\n");
        if (first == std::string_view::npos) return false;
        const auto last = body.find_last_not_of(" \t\r\n");
        body = body.substr(first, last - first + 1);
        if (body.empty() || body.size() > 20
            || !std::all_of(body.begin(), body.end(),
                            [](unsigned char c) { return c >= '0' && c <= '9'; })) {
            return false;
        }

        std::uint64_t parsed = 0;
        const auto [end, error] = std::from_chars(
            body.data(), body.data() + body.size(), parsed, 10);
        if (error != std::errc{} || end != body.data() + body.size()) return false;

        const auto universe = static_cast<std::uint8_t>(parsed >> 56);
        const auto accountType = static_cast<std::uint8_t>((parsed >> 52) & 0x0f);
        const auto instance = static_cast<std::uint32_t>((parsed >> 32) & 0x000f'ffff);
        const auto accountId = static_cast<std::uint32_t>(parsed & 0xffff'ffff);
        if (universe < 1 || universe > 5 || accountType != 1
            || instance != 1 || accountId == 0) {
            return false;
        }

        steamId = parsed;
        return true;
    }

    inline ParsedResponse ClassifyResponse(
        bool networkError,
        int status,
        std::string_view body) {
        ParsedResponse result;
        if (networkError || status != 200
            || !ParseSteamId64(body, result.steamId)) {
            result.steamId = 0;
            result.ttl = kNegativeTtl;
            return result;
        }
        result.ttl = kPositiveTtl;
        return result;
    }

    template <typename TimePoint>
    inline bool IsFresh(TimePoint expiresAt, TimePoint now) {
        return expiresAt > now;
    }

    inline bool CanEnqueue(
        bool queued,
        bool inFlight,
        std::size_t queueSize,
        std::size_t maxQueue) {
        return !queued && !inFlight && queueSize < maxQueue;
    }
}

#endif
