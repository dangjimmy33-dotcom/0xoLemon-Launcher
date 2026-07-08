// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "VehUtil.h"

namespace VehUtil {
    void ArmInt3(void* target) {
        DWORD oldProtect = 0;
        VirtualProtect(target, 1, PAGE_EXECUTE_READWRITE, &oldProtect);
        *static_cast<uint8_t*>(target) = 0xCC;
    }

    void RestoreByte(void* target, uint8_t original) {
        DWORD oldProtect = 0;
        VirtualProtect(target, 1, PAGE_EXECUTE_READWRITE, &oldProtect);
        *static_cast<uint8_t*>(target) = original;
    }
}
