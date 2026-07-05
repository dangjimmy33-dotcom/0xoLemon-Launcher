// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "hooks/client/NetPacket.h"
#include "config/LuaLoader.h"
#include "runtime/ManifestFetch.h"
#include "runtime/Logger.h"

namespace NetPacket::Handlers::DepotFallback {

bool HandleSend(const uint8_t* pBody, uint32_t cbBody,
                const uint8_t* pHdr, uint32_t cbHdr) {
    CContentServerDirectory_GetManifestRequestCode_Request req;
    if (!req.ParseFromArray(pBody, cbBody)) {
        LOG_PKTRT_WARN("{{\"evt\":\"DepotFallback\",\"act\":\"send\",\"err\":\"parse-fail\",\"size\":{}}}", cbBody);
        return false;
    }
    if (!req.has_depot_id() || !req.has_manifest_id()) {
        LOG_PKTRT_DEBUG("{{{{\"evt\":\"DepotFallback\",\"act\":\"send\",\"skip\":\"no-depot-or-manifest\"}}}}");
        return false;
    }
    const AppId_t depotId = req.depot_id();
    const uint64_t gid    = req.manifest_id();
    const AppId_t appId   = req.has_app_id() ? req.app_id() : 0;

    if (!LuaLoader::HasDepot(depotId)) {
        LOG_PKTRT_DEBUG("{{\"evt\":\"DepotFallback\",\"act\":\"send\",\"skip\":\"not-in-addappid\",\"depot\":{},\"gid\":{}}}",
                   depotId, gid);
        return false;
    }

    CMsgProtoBufHeader hdr;
    if (!hdr.ParseFromArray(pHdr, cbHdr) || !hdr.has_jobid_source()) {
        LOG_PKTRT_WARN("{{{{\"evt\":\"DepotFallback\",\"act\":\"send\",\"err\":\"no-jobid\"}}}}");
        return false;
    }
    const uint64_t jobId = hdr.jobid_source();

    LOG_PKTRT_INFO("{{\"evt\":\"DepotFallback\",\"act\":\"send\",\"depot\":{},\"gid\":{},\"app\":{},\"job\":{}}}",
               depotId, gid, appId, jobId);
    ManifestFetch::Submit(jobId, gid, appId, depotId);
    return false;
}

void HandleRecv(const uint8_t* pHdr, uint32_t cbHdr,
                const uint8_t* pBody, uint32_t cbBody) {
    CMsgProtoBufHeader hdr;
    if (!hdr.ParseFromArray(pHdr, cbHdr)) {
        LOG_PKTRT_WARN("{{{{\"evt\":\"DepotFallback\",\"act\":\"recv\",\"err\":\"header-parse-fail\"}}}}");
        return;
    }
    if (!hdr.has_jobid_target()) {
        LOG_PKTRT_DEBUG("{{{{\"evt\":\"DepotFallback\",\"act\":\"recv\",\"skip\":\"no-jobid\"}}}}");
        return;
    }
    const uint64_t jobId = hdr.jobid_target();

    auto resolved = ManifestFetch::Resolve(jobId);
    if (!resolved) {
        LOG_PKTRT_DEBUG("{{\"evt\":\"DepotFallback\",\"act\":\"recv\",\"skip\":\"no-resolve\",\"job\":{},\"size\":{},\"eresult\":{}}}",
                   jobId, cbBody, hdr.eresult());
        return;
    }

    hdr.set_eresult(static_cast<int32_t>(k_EResultOK));
    const size_t hdrSize = hdr.ByteSizeLong();
    if (hdrSize > kHdrCap || !hdr.SerializeToArray(s_rx.Hdr, kHdrCap)) {
        LOG_PKTRT_WARN("{{\"evt\":\"DepotFallback\",\"act\":\"recv\",\"err\":\"header-encode-fail\",\"size\":{}}}", hdrSize);
        return;
    }
    s_rx.HdrLen = static_cast<uint32_t>(hdrSize);

    CContentServerDirectory_GetManifestRequestCode_Response resp;
    resp.set_manifest_request_code(*resolved);
    const size_t bodySize = resp.ByteSizeLong();
    if (bodySize > kBodyCap || !resp.SerializeToArray(s_rx.Body, kBodyCap)) {
        LOG_PKTRT_WARN("{{\"evt\":\"DepotFallback\",\"act\":\"recv\",\"err\":\"body-encode-fail\",\"size\":{}}}", bodySize);
        return;
    }
    s_rx.BodyLen = static_cast<uint32_t>(bodySize);

    s_rx.PatchHdr = true;
    s_rx.PatchBody = true;
    LOG_PKTRT_INFO("{{\"evt\":\"DepotFallback\",\"act\":\"recv\",\"job\":{},\"code\":{},\"origSize\":{}}}",
               jobId, *resolved, cbBody);
}

} // namespace NetPacket::Handlers::DepotFallback
