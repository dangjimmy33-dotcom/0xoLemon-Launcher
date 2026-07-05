#pragma once
#include <Windows.h>
#include <cstdint>
#include <string>

namespace OxoPattern {

    // Pattern scanning using wildcards
    // Pattern format: "48 89 5C 24 08 ?? ?? 48 89"
    // ?? = wildcard (any byte)
    
    struct ScanResult {
        void* address;
        bool found;
    };

    // Scan module for byte pattern
    ScanResult ScanModule(HMODULE hModule, const char* pattern);
    
    // Scan range for byte pattern
    ScanResult ScanRange(uint8_t* start, size_t length, const char* pattern);
    
    // Get module base and size
    bool GetModuleInfo(HMODULE hModule, uint8_t** outBase, size_t* outSize);
    
    // Convert hex pattern string to byte array
    struct Pattern {
        uint8_t* bytes;
        bool* mask;  // true = compare, false = wildcard
        size_t length;
        
        ~Pattern() {
            delete[] bytes;
            delete[] mask;
        }
    };
    
    Pattern* ParsePattern(const char* patternStr);
    
    // Match pattern at specific address
    bool MatchPattern(uint8_t* address, const Pattern* pattern);

} // namespace OxoPattern
