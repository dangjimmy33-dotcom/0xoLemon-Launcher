// _0xoLemonCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "DecryptionKeyHook.h"
#include "hooks/Macros.h"
#include "core/entry.h"
#include "config/LuaLoader.h"

#include <cstring>
#include <filesystem>
#include <fstream>

namespace {

    void* g_configStoreObj = nullptr;

    static void CaptureStoreObj(void* pObject, EConfigStore storeType) {
        if (pObject && !g_configStoreObj && storeType == k_EConfigStoreUserLocal) {
            g_configStoreObj = pObject;
            LOG_DECRYPTIONKEYCH_INFO("captured user-local ConfigStore store={} object=0x{:X}",
                                     static_cast<int32>(storeType),
                                     reinterpret_cast<uint64_t>(pObject));
        }
    }

    // intercept depot decryption key fetches. if steam asks for a key we
    // know about, slap the hex blob directly into the output buffer.
    LM_HOOK(ConfigStoreGetBinary, int32, void* pObject, EConfigStore eConfigStore, const char* KeyName, char* Key, uint32 KeySize)
    {
        CaptureStoreObj(pObject, eConfigStore);

        if (!KeyName)
            return oConfigStoreGetBinary(pObject, eConfigStore, KeyName, Key, KeySize);

        const char* marker = strstr(KeyName, "\\DecryptionKey");
        if (!marker)
            return oConfigStoreGetBinary(pObject, eConfigStore, KeyName, Key, KeySize);

        // walk backwards to find the depot number segment
        const char* seg = marker;
        while (seg > KeyName && seg[-1] != '\\')
            --seg;
        if (seg == KeyName)
            return oConfigStoreGetBinary(pObject, eConfigStore, KeyName, Key, KeySize);

        char* end = nullptr;
        AppId_t depotId = static_cast<AppId_t>(strtoull(seg, &end, 10));
        if (!end || end != marker || depotId == 0)
            return oConfigStoreGetBinary(pObject, eConfigStore, KeyName, Key, KeySize);

        const auto& hexKey = LuaLoader::GetDecryptionKey(depotId);
        if (hexKey.empty())
            return oConfigStoreGetBinary(pObject, eConfigStore, KeyName, Key, KeySize);

        if (hexKey.size() > KeySize) {
            LOG_DECRYPTIONKEYCH_WARN("depot {} key won't fit ({} > {})", depotId, hexKey.size(), KeySize);
            return oConfigStoreGetBinary(pObject, eConfigStore, KeyName, Key, KeySize);
        }

        memcpy(Key, hexKey.data(), hexKey.size());
        LOG_DECRYPTIONKEYCH_INFO("fed key for depot {} ({} bytes)", depotId, hexKey.size());
        return static_cast<int32>(hexKey.size());
    }

    // Resolved via LM_BIND_STR for direct SetConfigStoreStringEx calls; no Detours hook needed.
    LM_HOOK(ConfigStoreSetString, bool, void* pObject, EConfigStore eConfigStore,
            const char* KeyName, const char* Value)
    {
        return oConfigStoreSetString(pObject, eConfigStore, KeyName, Value);
    }

    // After ConfigStore flushes to disk, re-apply our language to 480's ACF
    LM_HOOK(ConfigStoreFlushToDisk, bool, void* pObject, EConfigStore eConfigStore)
    {
        bool result = oConfigStoreFlushToDisk(pObject, eConfigStore);
        if (DecryptionKeyHook::g_spacewarLanguage[0]) {
            std::string acf = DecryptionKeyHook::FindAcfPath(kOnlineFixAppId);
            if (!acf.empty())
                DecryptionKeyHook::WriteAcfLanguage(acf, DecryptionKeyHook::g_spacewarLanguage);
        }
        return result;
    }

    std::vector<uint8_t> PullConfigStoreBlob(const std::string& keyName) {
        if (!g_configStoreObj || !oConfigStoreGetBinary) {
            LOG_DECRYPTIONKEYCH_WARN("PullConfigStoreBlob: no object or original, can't read binary");
            return {};
        }

        std::vector<uint8_t> blob(1024);
        int32 got = oConfigStoreGetBinary(g_configStoreObj, k_EConfigStoreUserLocal,
                                          keyName.c_str(),
                                          reinterpret_cast<char*>(blob.data()),
                                          static_cast<uint32>(blob.size()));
        if (got <= 0) {
            LOG_DECRYPTIONKEYCH_DEBUG("PullConfigStoreBlob: empty read for '{}'", keyName);
            return {};
        }

        blob.resize(got);
        LOG_DECRYPTIONKEYCH_DEBUG("PullConfigStoreBlob: '{}' -> {} bytes", keyName, got);
        return blob;
    }

}

namespace DecryptionKeyHook {

    void Install() {
        {
            static constexpr StringXRefSig kSetStringSigs[] = {
                {"CConfigStore::SetString", ""},
            };
            LM_BIND_STR(ConfigStoreSetString, kSetStringSigs, 1);
        }

        LM_TX_BEGIN();
        LM_INSTALL(ConfigStoreGetBinary);
        {
            static constexpr StringXRefSig kFlushSigs[] = {
                {"CConfigStore::FlushToDisk", ""},
            };
            LM_INSTALL_STR(ConfigStoreFlushToDisk, kFlushSigs, 1);
        }
        LM_TX_COMMIT();
    }

    void Uninstall() {
        LM_TX_BEGIN();
        LM_REMOVE(ConfigStoreGetBinary);
        LM_REMOVE(ConfigStoreFlushToDisk);
        LM_TX_COMMIT();
        oConfigStoreSetString = nullptr;
        g_configStoreObj = nullptr;
    }

    std::vector<uint8_t> GetCachedAppTicket(AppId_t appId) {
        // try ConfigStore first (fast when hook is live)
        std::string csKey = "apptickets\\" + std::to_string(appId);
        std::vector<uint8_t> ticket = PullConfigStoreBlob(csKey);
        if (!ticket.empty()) {
            LOG_DECRYPTIONKEYCH_DEBUG("cached ticket for AppId {} via CS ({} bytes)", appId, ticket.size());
            return ticket;
        }

        // ConfigStore can miss early in boot. Registry is the backup for
        // tickets Steam already wrote under the user.
        HKEY hKey;
        std::string regPath = "Software\\Valve\\Steam\\Apps\\" + std::to_string(appId);
        if (RegOpenKeyExA(HKEY_CURRENT_USER, regPath.c_str(), 0, KEY_READ, &hKey) == ERROR_SUCCESS) {
            std::vector<uint8_t> regVal(1024 * 1024);
            DWORD cb = static_cast<DWORD>(regVal.size());
            DWORD vt = 0;
            LSTATUS st = RegQueryValueExA(hKey, "AppTicket", nullptr, &vt, regVal.data(), &cb);
            RegCloseKey(hKey);
            if (st == ERROR_SUCCESS && vt == REG_BINARY && cb > 0) {
                regVal.resize(cb);
                LOG_DECRYPTIONKEYCH_INFO("cached ticket for AppId {} from registry ({} bytes)", appId, cb);
                return regVal;
            }
        }

        LOG_DECRYPTIONKEYCH_DEBUG("no cached ticket for AppId {}", appId);
        return {};
    }

    bool SetConfigStoreStringEx(const char* key, const char* value, EConfigStore store) {
        if (!g_configStoreObj || !oConfigStoreSetString || !key || !value)
            return false;
        return oConfigStoreSetString(g_configStoreObj, store, key, value);
    }

    char g_spacewarLanguage[64] = {};

    std::string BuildUserAppConfigBlob(const std::string& lang) {
        if (lang.empty()) return {};
        std::string blob;
        blob.reserve(32);
        blob += '\0';
        blob += "UserConfig";
        blob += '\0';
        blob += '\x01';  // 1 child
        blob += "language";
        blob += '\0';
        blob += lang;
        blob += '\0';
        blob += '\x08';  // end child
        blob += '\x08';  // end section
        return blob;
    }

    void StoreSpacewarLanguage(const char* lang) {
        if (lang) strncpy(g_spacewarLanguage, lang, sizeof(g_spacewarLanguage) - 1);
    }

    std::string FindAcfPath(AppId_t appId) {
        if (!SteamInstallPath[0]) return {};
        std::string acfName = "appmanifest_" + std::to_string(appId) + ".acf";
        auto checkDir = [&](const std::filesystem::path& d) -> std::string {
            auto p = d / acfName;
            std::error_code ec;
            return std::filesystem::exists(p, ec) ? p.string() : std::string{};
        };
        auto path = checkDir(std::filesystem::path(SteamInstallPath) / "steamapps");
        if (!path.empty()) return path;
        auto vdfPath = std::filesystem::path(SteamInstallPath) / "steamapps" / "libraryfolders.vdf";
        std::ifstream vdf(vdfPath);
        if (!vdf) { vdfPath = std::filesystem::path(SteamInstallPath) / "config" / "libraryfolders.vdf"; vdf.open(vdfPath); }
        if (!vdf) return {};
        std::string line;
        while (std::getline(vdf, line)) {
            auto pos = line.find("\"path\"");
            if (pos == std::string::npos) continue;
            auto q1 = line.find('"', pos + 6);
            if (q1 == std::string::npos) continue;
            auto q2 = line.find('"', q1 + 1);
            if (q2 == std::string::npos) continue;
            std::string lib = line.substr(q1 + 1, q2 - q1 - 1);
            if (lib.size() < 3 || lib[1] != ':' || lib[2] != '\\') continue;
            path = checkDir(std::filesystem::path(lib) / "steamapps");
            if (!path.empty()) return path;
        }
        return {};
    }

    std::string ReadAcfLanguage(const std::string& acfPath) {
        std::ifstream f(acfPath);
        if (!f) return {};
        std::string line;
        bool inUC = false;
        int depth = 0;
        while (std::getline(f, line)) {
            if (line.find("\"UserConfig\"") != std::string::npos) { inUC = true; depth = 0; continue; }
            if (!inUC) continue;
            for (char c : line) { if (c == '{') ++depth; else if (c == '}') --depth; }
            if (depth < 0) return {};
            if (depth > 0) {
                auto lp = line.find("\"language\"");
                if (lp != std::string::npos) {
                    auto q1 = line.find('"', lp + 10);
                    if (q1 != std::string::npos) {
                        auto q2 = line.find('"', q1 + 1);
                        if (q2 != std::string::npos && q2 > q1 + 1)
                            return line.substr(q1 + 1, q2 - q1 - 1);
                    }
                }
            }
        }
        return {};
    }

    void WriteAcfLanguage(const std::string& acfPath, const std::string& lang) {
        if (lang.empty()) return;
        std::ifstream in(acfPath);
        if (!in) {
            std::ofstream out(acfPath, std::ios::trunc);
            if (!out) return;
            out << "\"AppState\"\n{\n\t\"appid\"\t\t\"480\"\n"
                   "\t\"name\"\t\t\"Spacewar\"\n"
                   "\t\"StateFlags\"\t\t\"4\"\n"
                   "\t\"installdir\"\t\t\"Spacewar\"\n"
                   "\t\"UserConfig\"\n\t{\n\t\t\"language\"\t\t\""
                << lang << "\"\n\t}\n}\n";
            out.close();
            return;
        }
        std::string content((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
        in.close();
        auto uc = content.find("\"UserConfig\"");
        if (uc == std::string::npos) return;
        auto lp = content.find("\"language\"", uc);
        if (lp != std::string::npos) {
            auto q1 = content.find('"', lp + 10);
            if (q1 != std::string::npos) {
                auto q2 = content.find('"', q1 + 1);
                if (q2 != std::string::npos)
                    content.replace(q1 + 1, q2 - q1 - 1, lang);
            }
        }
        std::ofstream out(acfPath, std::ios::trunc);
        if (out) { out.write(content.data(), static_cast<std::streamsize>(content.size())); out.close(); }
    }

}
