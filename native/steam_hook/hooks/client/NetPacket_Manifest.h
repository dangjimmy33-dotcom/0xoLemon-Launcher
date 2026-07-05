// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include "Steam/Types.h"

namespace NetPacket::Handlers::DepotFallback {
    bool HandleSend(const uint8_t* pBody, uint32_t cbBody,
                    const uint8_t* pHdr, uint32_t cbHdr);
    void HandleRecv(const uint8_t* pHdr, uint32_t cbHdr,
                    const uint8_t* pBody, uint32_t cbBody);
}
