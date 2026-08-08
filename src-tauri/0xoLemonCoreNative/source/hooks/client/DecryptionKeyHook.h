// _0xoLemonCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include "core/entry.h"

namespace DecryptionKeyHook {
    void Install();
    void Uninstall();

    extern char g_spacewarLanguage[64];
    std::vector<uint8_t> GetCachedAppTicket(AppId_t appId);
    void StoreSpacewarLanguage(const char* lang);
    bool SetConfigStoreStringEx(const char* key, const char* value, EConfigStore store);
    std::string BuildUserAppConfigBlob(const std::string& lang);
    std::string ReadAcfLanguage(const std::string& acfPath);
    std::string FindAcfPath(AppId_t appId);
    void WriteAcfLanguage(const std::string& acfPath, const std::string& lang);
}
