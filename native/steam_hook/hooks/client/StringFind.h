// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include <windows.h>

#include "hooks/SigTypes.h"

namespace StringFind {
    // Finds a function by tracing which code references a known string literal.
    // Step 1: scans all non-executable sections (e.g. .rdata) for every address
    //         that holds the exact bytes of targetStr followed by a null terminator.
    // Step 2: scans the first executable section (.text) for RIP-relative LEA instructions
    //         (opcode pattern: REX 8D [mod=00 r/m=101] disp32) whose computed target
    //         address matches one of the string addresses found in step 1.
    //         The `occurrence` parameter selects which LEA hit to use (1 = first, 2 = second, ...).
    // Step 3: looks up the instruction's offset in the .pdata exception directory (binary search
    //         on RUNTIME_FUNCTION BeginAddress), which gives the start of the enclosing function.
    // Returns nullptr if the string is not found, no LEA references it, or .pdata has no entry.
    void* FindFunction(HMODULE hMod, const char* targetStr, int occurrence = 1);
}
