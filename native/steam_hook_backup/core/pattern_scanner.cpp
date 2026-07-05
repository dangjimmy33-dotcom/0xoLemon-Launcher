#include "pattern_scanner.h"
#include <cstring>
#include <cctype>
#include <Psapi.h>

#pragma comment(lib, "psapi.lib")

namespace OxoPattern {

Pattern* ParsePattern(const char* patternStr) {
    // Count bytes
    size_t len = strlen(patternStr);
    size_t byteCount = 0;
    for (size_t i = 0; i < len; i++) {
        if (isxdigit(patternStr[i])) byteCount++;
    }
    byteCount /= 2;
    
    if (byteCount == 0) return nullptr;
    
    Pattern* pattern = new Pattern();
    pattern->bytes = new uint8_t[byteCount];
    pattern->mask = new bool[byteCount];
    pattern->length = byteCount;
    
    size_t idx = 0;
    for (size_t i = 0; i < len && idx < byteCount; ) {
        // Skip whitespace
        while (i < len && isspace(patternStr[i])) i++;
        if (i >= len) break;
        
        // Check for wildcard
        if (patternStr[i] == '?' && i + 1 < len && patternStr[i + 1] == '?') {
            pattern->bytes[idx] = 0;
            pattern->mask[idx] = false;
            idx++;
            i += 2;
        } else if (isxdigit(patternStr[i]) && i + 1 < len && isxdigit(patternStr[i + 1])) {
            char byteStr[3] = {patternStr[i], patternStr[i + 1], '\0'};
            pattern->bytes[idx] = (uint8_t)strtol(byteStr, nullptr, 16);
            pattern->mask[idx] = true;
            idx++;
            i += 2;
        } else {
            i++;
        }
    }
    
    return pattern;
}

bool MatchPattern(uint8_t* address, const Pattern* pattern) {
    for (size_t i = 0; i < pattern->length; i++) {
        if (pattern->mask[i]) {
            if (address[i] != pattern->bytes[i]) {
                return false;
            }
        }
    }
    return true;
}

bool GetModuleInfo(HMODULE hModule, uint8_t** outBase, size_t* outSize) {
    if (!hModule) return false;
    
    MODULEINFO modInfo;
    if (!GetModuleInformation(GetCurrentProcess(), hModule, &modInfo, sizeof(modInfo))) {
        return false;
    }
    
    *outBase = (uint8_t*)modInfo.lpBaseOfDll;
    *outSize = modInfo.SizeOfImage;
    return true;
}

ScanResult ScanRange(uint8_t* start, size_t length, const char* patternStr) {
    ScanResult result = {nullptr, false};
    
    Pattern* pattern = ParsePattern(patternStr);
    if (!pattern) return result;
    
    for (size_t i = 0; i <= length - pattern->length; i++) {
        if (MatchPattern(start + i, pattern)) {
            result.address = start + i;
            result.found = true;
            break;
        }
    }
    
    delete pattern;
    return result;
}

ScanResult ScanModule(HMODULE hModule, const char* patternStr) {
    ScanResult result = {nullptr, false};
    
    uint8_t* base;
    size_t size;
    if (!GetModuleInfo(hModule, &base, &size)) {
        return result;
    }
    
    return ScanRange(base, size, patternStr);
}

} // namespace OxoPattern
