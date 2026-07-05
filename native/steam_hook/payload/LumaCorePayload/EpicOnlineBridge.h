// LumaCorePayload — injected into game processes for EOS bridge.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include <windows.h>

namespace EosBridge {
    void InstallOn(HMODULE eosModule);
}
