#pragma once

// Steam function byte patterns - Multi-version support
// Patterns are ordered by preference (most specific first)

namespace SteamPatterns {

    // Pattern set structure
    struct PatternSet {
        const char* primary;
        const char* fallback1;
        const char* fallback2;
        const char* fallback3;
    };

    // LoadPackage - Package subscription loader
    // Signature: bool LoadPackage(PackageInfo*, uint8*, int32, void*)
    constexpr PatternSet LoadPackage = {
        // v1: Specific pattern with full prologue
        "48 89 5C 24 ?? 48 89 6C 24 ?? 48 89 74 24 ?? 57 48 83 EC ?? 48 8B F9",
        // v2: Shorter pattern (more flexible)
        "48 89 5C 24 ?? 48 89 6C 24 ?? 48 89 74 24 ?? 57",
        // v3: Minimal pattern (very flexible)
        "48 89 5C 24 ?? 48 89 6C 24 ?? 48 89 74",
        // v4: Ultra-minimal (last resort)
        "48 89 5C 24 ?? 48 89 6C 24"
    };

    // CheckAppOwnership - Ownership verification
    // Signature: bool CheckAppOwnership(void*, AppId_t, AppOwnership*)
    constexpr PatternSet CheckAppOwnership = {
        // v1: Full pattern
        "40 53 48 83 EC ?? 48 8B D9 E8 ?? ?? ?? ?? 84 C0",
        // v2: Without tail check
        "40 53 48 83 EC ?? 48 8B D9 E8",
        // v3: Minimal prologue
        "40 53 48 83 EC ?? 48 8B D9",
        // v4: Ultra-minimal
        "40 53 48 83 EC ??"
    };

    // GetSubscribedApps - Library app list
    // Signature: uint32 GetSubscribedApps(void*, uint32*, uint32, uint8)
    constexpr PatternSet GetSubscribedApps = {
        // v1: Full pattern
        "48 89 5C 24 ?? 48 89 6C 24 ?? 48 89 74 24 ?? 57 41 56 41 57 48 83 EC ?? 44 8B F9",
        // v2: Shorter
        "48 89 5C 24 ?? 48 89 6C 24 ?? 48 89 74 24 ?? 57 41 56 41 57",
        // v3: Minimal
        "48 89 5C 24 ?? 48 89 6C 24 ?? 48 89 74 24 ?? 57",
        // v4: Ultra-minimal
        "48 89 5C 24 ?? 48 89 6C 24"
    };

    // CUtlMemoryGrow - Vector growth (CRITICAL)
    // Signature: void* CUtlMemoryGrow(CUtlVector*, int)
    constexpr PatternSet CUtlMemoryGrow = {
        // v1: Full pattern
        "48 89 5C 24 ?? 57 48 83 EC ?? 8B 41 ?? 48 8B F9 03 C2",
        // v2: Without arithmetic
        "48 89 5C 24 ?? 57 48 83 EC ?? 8B 41",
        // v3: Minimal
        "48 89 5C 24 ?? 57 48 83 EC",
        // v4: Ultra-minimal
        "48 89 5C 24 ?? 57"
    };

    // Helper: Try all patterns in a set
    inline const char* GetPattern(const PatternSet& set, int index) {
        switch (index) {
            case 0: return set.primary;
            case 1: return set.fallback1;
            case 2: return set.fallback2;
            case 3: return set.fallback3;
            default: return nullptr;
        }
    }

} // namespace SteamPatterns

// Pattern update instructions:
//
// 1. Get Steam build number:
//    - Check steam.exe properties or use SteamDB
//
// 2. Use x64dbg or IDA:
//    - Load steamclient64.dll
//    - Search for string references like "Package", "AppOwnership", "Subscribed"
//    - Find the functions and copy first 16-24 bytes
//    - Replace ?? for bytes that might change (offsets, etc.)
//
// 3. Test patterns:
//    - Run Steam with hooks
//    - Check DebugView for "[OxoHook] Found XXX at 0x..."
//    - If pattern fails, try fallback or update pattern
//
// 4. Share patterns:
//    - Post working patterns in GitHub issues with Steam build number
