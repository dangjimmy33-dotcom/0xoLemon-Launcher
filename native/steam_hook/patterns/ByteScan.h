// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

// Public name resolver for hook installers in the TOML-only world. ByteSearch
// reads the cached PatternResult for the module, finds the function name in
// the parsed entries (with a "KeyValues_" alias retry for older pattern
// uploads), and returns module-base + rva. On miss it logs one warning that
// names both the hook and the module, then returns nullptr so the caller can
// mark the miss without crashing.
//
// PatchMemoryBytes is the second public entry point - a small wrapper around
// VirtualProtect / memcpy / FlushInstructionCache used by the CmdUser and
// CmdUtils restore paths.

#include <windows.h>

// Resolves funcName against the TOML PatternResult cached for module.
// Returns module + rva on a direct or "KeyValues_"-prefixed hit. Returns
// nullptr (and logs once) when the name is absent or when no PatternResult
// has been registered for the module yet.
void* ByteSearch(HMODULE module, const char* funcName);

// Overwrites nSize bytes at pAddress with the bytes from pNewBytes.
// VirtualProtect makes the page writable, memcpy lands the bytes,
// FlushInstructionCache nudges the CPU to drop any stale decoded ops.
// Returns 1 on success, 0 if VirtualProtect fails.
int PatchMemoryBytes(void* pAddress, const void* pNewBytes, SIZE_T nSize);

