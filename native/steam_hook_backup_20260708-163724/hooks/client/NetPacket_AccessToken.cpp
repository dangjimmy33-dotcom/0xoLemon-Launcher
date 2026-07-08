// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "hooks/client/NetPacket.h"
#include "config/LuaLoader.h"
#include "runtime/Logger.h"

namespace NetPacket::Handlers::AccessToken {

bool HandleSend(const uint8_t* pBody, uint32_t cbBody) {
    CMsgClientPICSProductInfoRequest req;
    if (!req.ParseFromArray(pBody, cbBody)) {
        LOG_PKTRT_WARN("{{{{\"evt\":\"PICS\",\"act\":\"parse-fail\"}}}}");
        return false;
    }
    LOG_PKTRT_DEBUG("{{\"evt\":\"PICS\",\"act\":\"original\",\"body\":{}}}", req.DebugString());

    bool needsPatch = false;
    for (const auto& app : req.apps()) {
        if (LuaLoader::HasDepot(app.appid()) && LuaLoader::GetAccessToken(app.appid())) {
            needsPatch = true;
            LOG_PKTRT_DEBUG("{{\"evt\":\"PICS\",\"act\":\"need-patch\",\"appId\":{}}}", app.appid());
            break;
        }
    }
    if (!needsPatch) {
        LOG_PKTRT_TRACE("{{{{\"evt\":\"PICS\",\"act\":\"skip\"}}}}");
        return false;
    }

    int injected = 0, noToken = 0, notAddAppId = 0;
    for (auto& app : *req.mutable_apps()) {
        if (LuaLoader::HasDepot(app.appid())) {
            uint64_t token = LuaLoader::GetAccessToken(app.appid());
            if (token) {
                LOG_PKTRT_DEBUG("{{\"evt\":\"PICS\",\"act\":\"inject\",\"appId\":{},\"old\":{},\"new\":{}}}", app.appid(),
                           app.has_access_token() ? std::to_string(app.access_token()) : "absent",
                           token);
                app.set_access_token(token);
                ++injected;
            } else {
                LOG_PKTRT_WARN("{{\"evt\":\"PICS\",\"act\":\"skip-notoken\",\"appId\":{}}}", app.appid());
                ++noToken;
            }
        } else {
            ++notAddAppId;
        }
    }
    LOG_PKTRT_DEBUG("{{\"evt\":\"PICS\",\"act\":\"summary\",\"injected\":{},\"noToken\":{},\"notAddAppId\":{},\"total\":{}}}",
               injected, noToken, notAddAppId, req.apps_size());

    s_tx.BodyLen = static_cast<uint32_t>(req.ByteSizeLong());
    if (s_tx.BodyLen > kBodyCap) {
        LOG_PKTRT_WARN("{{\"evt\":\"PICS\",\"act\":\"overflow\",\"size\":{}}}", s_tx.BodyLen);
        return false;
    }
    if (!req.SerializeToArray(s_tx.Body, kBodyCap)) {
        LOG_PKTRT_WARN("{{{{\"evt\":\"PICS\",\"act\":\"encode-fail\"}}}}");
        return false;
    }

    LOG_PKTRT_DEBUG("{{\"evt\":\"PICS\",\"act\":\"modified\",\"body\":{}}}", req.DebugString());
    return true;
}

} // namespace NetPacket::Handlers::AccessToken
