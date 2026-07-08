// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "EticketFetcher.h"

#include "runtime/RuntimeHttp.h"
#include "config/LuaLoader.h"
#include "runtime/Logger.h"

#include <cstring>
#include <mutex>
#include <string>
#include <string_view>
#include <unordered_map>

namespace EticketFetcher {
namespace {

    // backend url comes from lua seteticketurl(), empty means disabled
    // POST {app_id, nonce(hex)} → backend mints against pool account
    // response {eticket, appticket} both hex strings
    // only successful fetches are cached per app id

    struct TicketPair {
        std::vector<uint8_t> eticket;
        std::vector<uint8_t> ownership;
    };

    std::mutex g_lock;
    std::unordered_map<AppId_t, TicketPair> g_cache;

    int HexVal(char c) {
        if (c >= '0' && c <= '9') return c - '0';
        if (c >= 'a' && c <= 'f') return c - 'a' + 10;
        if (c >= 'A' && c <= 'F') return c - 'A' + 10;
        return -1;
    }

    bool DecodeHex(std::string_view hex, std::vector<uint8_t>& out) {
        if (hex.size() % 2) return false;
        out.clear();
        out.reserve(hex.size() / 2);
        for (size_t i = 0; i < hex.size(); i += 2) {
            int hi = HexVal(hex[i]), lo = HexVal(hex[i + 1]);
            if (hi < 0 || lo < 0) return false;
            out.push_back(static_cast<uint8_t>((hi << 4) | lo));
        }
        return true;
    }

    static const char kHexTable[] = "0123456789ABCDEF";

    std::string EncodeHex(std::span<const uint8_t> data) {
        std::string s;
        s.reserve(data.size() * 2);
        for (uint8_t b : data) {
            s.push_back(kHexTable[b >> 4]);
            s.push_back(kHexTable[b & 0x0F]);
        }
        return s;
    }

    // pull a "key":"value" string field from backend json response
    bool PullField(std::string_view json, std::string_view key, std::string& out) {
        std::string pat = "\"" + std::string(key) + "\"";
        size_t pos = json.find(pat);
        if (pos == std::string_view::npos) return false;
        size_t col = json.find(':', pos + pat.size());
        if (col == std::string_view::npos) return false;
        size_t q1 = json.find('"', col + 1);
        if (q1 == std::string_view::npos) return false;
        size_t delim = json.find_first_of(",}", col + 1);
        if (delim != std::string_view::npos && q1 > delim) return false;
        size_t q2 = json.find('"', q1 + 1);
        if (q2 == std::string_view::npos) return false;
        out.assign(json.data() + q1 + 1, q2 - q1 - 1);
        return !out.empty();
    }

    bool DoFetch(AppId_t appId, std::span<const uint8_t> nonce, TicketPair& out) {
        {
            std::lock_guard<std::mutex> hold(g_lock);
            auto it = g_cache.find(appId);
            if (it != g_cache.end()) { out = it->second; return true; }
        }

        const std::string& backendUrl = LuaLoader::GetEticketUrl();
        if (backendUrl.empty()) return false;

        std::string nonceHex = EncodeHex(nonce);
        std::string payload =
            "{\"app_id\":\"" + std::to_string(appId) + "\",\"nonce\":\"" + nonceHex + "\"}";

        auto resp = RuntimeHttp::Post(backendUrl, payload, {"Content-Type: application/json"});

        if (resp.networkError || resp.status != 200) {
            LOG_ETICKETCH_WARN("EticketFetcher: backend fail app={} status={} err={} (falling back)",
                               appId, resp.status, resp.networkError ? 1 : 0);
            return false;
        }

        TicketPair pair;
        std::string hex;
        if (PullField(resp.body, "eticket", hex))
            DecodeHex(hex, pair.eticket);
        if (PullField(resp.body, "appticket", hex))
            DecodeHex(hex, pair.ownership);

        if (pair.eticket.empty() && pair.ownership.empty()) {
            LOG_ETICKETCH_WARN("EticketFetcher: no usable tickets from backend app={} body_bytes={}",
                               appId, resp.body.size());
            return false;
        }

        {
            std::lock_guard<std::mutex> hold(g_lock);
            g_cache[appId] = pair;
            out = pair;
        }

        LOG_ETICKETCH_INFO("EticketFetcher: minted app={} eticket={}b ownership={}b nonce={}b",
                           appId, pair.eticket.size(), pair.ownership.size(), nonce.size());
        return true;
    }

} // namespace

std::optional<std::vector<uint8_t>> MintEticket(AppId_t appId, std::span<const uint8_t> nonce) {
    TicketPair t;
    if (!DoFetch(appId, nonce, t) || t.eticket.empty()) return std::nullopt;
    return t.eticket;
}

std::optional<std::vector<uint8_t>> MintOwnership(AppId_t appId, std::span<const uint8_t> nonce) {
    TicketPair t;
    if (!DoFetch(appId, nonce, t) || t.ownership.empty()) return std::nullopt;
    return t.ownership;
}

}
