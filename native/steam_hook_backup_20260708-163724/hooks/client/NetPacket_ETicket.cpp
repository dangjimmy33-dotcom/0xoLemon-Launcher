// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "hooks/client/NetPacket.h"
#include "config/LuaLoader.h"
#include "runtime/Ticket.h"
#include "runtime/Logger.h"

namespace NetPacket::Handlers::ETicket {

void HandleEncryptedAppTicketResponse(const uint8_t* pBody, uint32_t cbBody) {
    CMsgClientRequestEncryptedAppTicketResponse resp;
    if (!resp.ParseFromArray(pBody, cbBody)) {
        LOG_PKTRT_WARN("{{{{\"evt\":\"ETicket\",\"act\":\"recv\",\"err\":\"parse-fail\"}}}}");
        return;
    }
    LOG_PKTRT_DEBUG("{{\"evt\":\"ETicket\",\"act\":\"recv\",\"original\":{}}}", resp.DebugString());

    if (resp.eresult() == k_EResultOK) return;
    if (!LuaLoader::HasDepot(resp.app_id())) return;

    auto ticket = Ticket::GetEncryptedTicketFromRegistry(resp.app_id());
    if (ticket.empty()) return;

    if (!resp.mutable_encrypted_app_ticket()->ParseFromArray(
            ticket.data(), static_cast<int>(ticket.size()))) {
        LOG_PKTRT_WARN("{{{{\"evt\":\"ETicket\",\"act\":\"recv\",\"err\":\"ticket-parse-fail\"}}}}");
        return;
    }

    resp.set_eresult(k_EResultOK);

    auto encSize = resp.ByteSizeLong();
    if (encSize > sizeof(s_rx.Body)) {
        LOG_PKTRT_WARN("{{\"evt\":\"ETicket\",\"act\":\"recv\",\"err\":\"overflow\",\"size\":{}}}", encSize);
        return;
    }
    if (!resp.SerializeToArray(s_rx.Body, sizeof(s_rx.Body))) {
        LOG_PKTRT_WARN("{{{{\"evt\":\"ETicket\",\"act\":\"recv\",\"err\":\"encode-fail\"}}}}");
        return;
    }

    LOG_PKTRT_DEBUG("{{\"evt\":\"ETicket\",\"act\":\"recv\",\"modified\":{}}}", resp.DebugString());

    s_rx.BodyLen = static_cast<uint32_t>(encSize);
    s_rx.PatchBody = true;
}

} // namespace NetPacket::Handlers::ETicket
