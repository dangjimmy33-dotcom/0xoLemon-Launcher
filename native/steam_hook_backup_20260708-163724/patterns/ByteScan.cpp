// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "ByteScan.h"

#include "patterns/PatternFetcher.h"
#include "runtime/Logger.h"

#include <windows.h>
#include <psapi.h>

#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

namespace {

// Linear walk over the parsed TOML entries. The list is small (one TOML per
// module, a few dozen entries) and the lookup runs once per hook installer
// call, so a vector walk is cheaper than building a hash map.
const PatternFetcher::TomlEntry* FindEntry(
    const std::vector<PatternFetcher::TomlEntry>& entries,
    const char* name)
{
    for (const auto& e : entries) {
        if (e.name == name) return &e;
    }
    return nullptr;
}

// Friendly basename for the miss log. The full path is noisy and a reader
// only ever cares about steamclient64.dll vs steamui.dll vs lcoverlay.dll.
std::string ModuleBasename(HMODULE module) {
    char buf[MAX_PATH] = {};
    DWORD n = GetModuleFileNameA(module, buf, MAX_PATH);
    if (n == 0 || n == MAX_PATH) return "<unknown-module>";
    const char* slash = std::strrchr(buf, '\\');
    return slash ? std::string(slash + 1) : std::string(buf);
}

// Convert a single hex character to its numeric value, -1 on bad input.
int HexDigit(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

// Parse "AA BB ?? CC" into byte/mask vectors. Wildcards `??` mark positions
// where the live module byte is allowed to differ from the analyzer
// snapshot. Returns false on any malformed token.
bool ParseSig(const char* str, std::vector<std::uint8_t>& bytes,
              std::vector<std::uint8_t>& mask) {
    bytes.clear();
    mask.clear();
    for (const char* p = str; *p; ) {
        if (*p == ' ' || *p == '\t' || *p == ',') { ++p; continue; }
        if (p[0] == '?' && p[1] == '?') {
            bytes.push_back(0);
            mask.push_back(0);
            p += 2;
            continue;
        }
        int hi = HexDigit(p[0]);
        int lo = HexDigit(p[1]);
        if (hi < 0 || lo < 0) return false;
        bytes.push_back(static_cast<std::uint8_t>((hi << 4) | lo));
        mask.push_back(1);
        p += 2;
    }
    return !bytes.empty();
}

// Scan the loaded module image for the pattern, using memchr on the first
// concrete (non-wildcard) byte as an anchor. memchr is far faster than a
// byte-by-byte walk on a 26 MB DLL. Returns the address of the first hit.
void* ScanModule(HMODULE module,
                 const std::vector<std::uint8_t>& bytes,
                 const std::vector<std::uint8_t>& mask) {
    if (bytes.empty()) return nullptr;

    MODULEINFO modInfo{};
    if (!GetModuleInformation(GetCurrentProcess(), module,
                              &modInfo, sizeof(MODULEINFO))) {
        return nullptr;
    }

    const auto* base    = static_cast<const std::uint8_t*>(modInfo.lpBaseOfDll);
    const SIZE_T imgSize = modInfo.SizeOfImage;
    const SIZE_T patLen  = bytes.size();
    if (imgSize < patLen) return nullptr;

    // Find the first concrete byte to use as the memchr anchor.
    SIZE_T  anchorOff  = SIZE_T(-1);
    std::uint8_t anchorByte = 0;
    for (SIZE_T k = 0; k < patLen; ++k) {
        if (mask[k]) { anchorOff = k; anchorByte = bytes[k]; break; }
    }
    if (anchorOff == SIZE_T(-1)) return nullptr;  // all wildcards

    const SIZE_T scanEnd = imgSize - patLen;
    const auto* scanFrom = base + anchorOff;
    SIZE_T left = scanEnd + 1;

    while (left) {
        const auto* aHit = static_cast<const std::uint8_t*>(
            std::memchr(scanFrom, anchorByte, left));
        if (!aHit) break;

        const auto* start = aHit - anchorOff;
        bool ok = true;
        for (SIZE_T j = 0; j < patLen; ++j) {
            if (mask[j] && start[j] != bytes[j]) { ok = false; break; }
        }
        if (ok) return const_cast<std::uint8_t*>(start);

        SIZE_T consumed = static_cast<SIZE_T>(aHit + 1 - scanFrom);
        if (consumed >= left) break;
        left    -= consumed;
        scanFrom = aHit + 1;
    }
    return nullptr;
}

} // namespace

// Resolve a function in the live module. Primary path: use the TOML RVA
// directly (fast and works even after prior hooks have overwritten the
// function prologue). Fall back to a full-module byte scan only when the
// TOML carries no RVA.
void* ByteSearch(HMODULE module, const char* funcName) {
    if (!module || !funcName) return nullptr;

    const auto& result = PatternFetcher::Get(module);

    // Direct lookup first; on miss retry with the legacy "KeyValues_" prefix
    // older TOML uploads use for KeyValues hooks.
    const PatternFetcher::TomlEntry* entry = FindEntry(result.entries, funcName);
    if (!entry) {
        std::string aliased = "KeyValues_";
        aliased += funcName;
        entry = FindEntry(result.entries, aliased.c_str());
    }

    if (!entry) {
        LOG_WARN("ByteSearch: '{}' missing from TOML for {}",
                 funcName, ModuleBasename(module));
        return nullptr;
    }

    // RVA-first resolution: when the TOML publishes an RVA, compute the
    // address directly from module_base + rva. This is correct even when
    // earlier hooks have already overwritten the function prologue with
    // Detours trampolines, because the RVA still points to the right place.
    if (entry->rva != 0) {
        MODULEINFO modInfo{};
        if (GetModuleInformation(GetCurrentProcess(), module, &modInfo, sizeof(MODULEINFO))) {
            if (entry->rva < modInfo.SizeOfImage) {
                const auto* base = static_cast<const std::uint8_t*>(modInfo.lpBaseOfDll);
                return const_cast<std::uint8_t*>(base + entry->rva);
            }
        }
    }

    // Fall back to full-module byte scan when RVA is unavailable.
    if (entry->sig.empty()) {
        LOG_WARN("ByteSearch: '{}' TOML entry has empty sig for {}",
                 funcName, ModuleBasename(module));
        return nullptr;
    }

    std::vector<std::uint8_t> bytes, mask;
    if (!ParseSig(entry->sig.c_str(), bytes, mask)) {
        LOG_WARN("ByteSearch: '{}' bad sig string for {}",
                 funcName, ModuleBasename(module));
        return nullptr;
    }

    void* hit = ScanModule(module, bytes, mask);
    if (!hit) {
        LOG_WARN("ByteSearch: '{}' pattern not found in {}",
                 funcName, ModuleBasename(module));
    }
    return hit;
}

// Writes nSize bytes from pNewBytes into the memory at pAddress.
// The target is typically inside a loaded DLL's code section, which is
// read+execute but not writable. VirtualProtect temporarily marks the
// page as PAGE_EXECUTE_READWRITE, the bytes are copied, then
// FlushInstructionCache tells the CPU to discard any cached decoded
// instructions at that range so the patched bytes take effect immediately.
int PatchMemoryBytes(void* pAddress, const void* pNewBytes, SIZE_T nSize) {
    if (!pAddress || !pNewBytes || nSize == 0) return 0;
    DWORD oldProtect = 0;
    if (!VirtualProtect(pAddress, nSize, PAGE_EXECUTE_READWRITE, &oldProtect))
        return 0;
    memcpy(pAddress, pNewBytes, nSize);
    FlushInstructionCache(GetCurrentProcess(), pAddress, nSize);
    DWORD tmp = 0;
    VirtualProtect(pAddress, nSize, oldProtect, &tmp);
    return 1;
}
