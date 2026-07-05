// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "hooks/client/NetPacket.h"
#include "runtime/Logger.h"

namespace NetPacket::Handlers::FamilySharing {

void ClearBody(const uint8_t*, uint32_t) {
    LOG_PKTRT_DEBUG("{{{{\"evt\":\"FamilySharing\",\"act\":\"clear\"}}}}");
    s_rx.BodyLen = 0;
    s_rx.PatchBody = true;
}

} // namespace NetPacket::Handlers::FamilySharing
