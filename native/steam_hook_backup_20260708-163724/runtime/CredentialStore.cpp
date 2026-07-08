// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "CredentialStore.h"
#include "Logger.h"

#define WIN32_LEAN_AND_MEAN
#include <windows.h>

namespace CredentialStore {

    namespace {
        constexpr const char* kRegKey = "Software\\Valve\\Steam\\Apps";

        std::string AppRegPath(AppId_t appId) {
            return std::string(kRegKey) + "\\" + std::to_string(appId);
        }

        Status LastErrorToStatus() {
            DWORD err = GetLastError();
            if (err == ERROR_FILE_NOT_FOUND || err == ERROR_PATH_NOT_FOUND)
                return Status::NotFound;
            if (err == ERROR_ACCESS_DENIED)
                return Status::AccessDenied;
            return Status::Error;
        }
    }

    const char* ToString(Status s) {
        switch (s) {
        case Status::Ok:         return "Ok";
        case Status::NotFound:   return "NotFound";
        case Status::AccessDenied: return "AccessDenied";
        case Status::Error:      return "Error";
        }
        return "?";
    }

    Status ReadSteamId(AppId_t appId, uint64_t& outSteamId) {
        const std::string path = AppRegPath(appId);
        DWORD type = 0, size = 0;
        LSTATUS st = RegGetValueA(HKEY_CURRENT_USER, path.c_str(), "SteamID",
                                   RRF_RT_REG_SZ, &type, nullptr, &size);
        if (st != ERROR_SUCCESS || size == 0)
            return LastErrorToStatus();

        std::string buf(size, '\0');
        st = RegGetValueA(HKEY_CURRENT_USER, path.c_str(), "SteamID",
                           RRF_RT_REG_SZ, &type, buf.data(), &size);
        if (st != ERROR_SUCCESS || type != REG_SZ)
            return Status::Error;

        while (!buf.empty() && buf.back() == '\0')
            buf.pop_back();
        if (buf.empty()) return Status::NotFound;

        char* end = nullptr;
        uint64_t val = strtoull(buf.c_str(), &end, 10);
        if (end == buf.c_str() || val == 0) return Status::NotFound;
        outSteamId = val;
        return Status::Ok;
    }

    Status WriteSteamId(AppId_t appId, uint64_t steamId) {
        if (appId == 0 || steamId == 0) return Status::Error;
        const std::string path = AppRegPath(appId);
        HKEY hKey = nullptr;
        DWORD disp = 0;
        LSTATUS st = RegCreateKeyExA(HKEY_CURRENT_USER, path.c_str(), 0, nullptr,
                                      REG_OPTION_NON_VOLATILE, KEY_SET_VALUE,
                                      nullptr, &hKey, &disp);
        if (st != ERROR_SUCCESS) return LastErrorToStatus();

        std::string val = std::to_string(steamId);
        st = RegSetValueExA(hKey, "SteamID", 0, REG_SZ,
                             reinterpret_cast<const BYTE*>(val.c_str()),
                             static_cast<DWORD>(val.size() + 1));
        RegCloseKey(hKey);
        LOG_MISC_INFO("CredentialStore: wrote SteamID AppId={} value={}", appId, steamId);
        return (st == ERROR_SUCCESS) ? Status::Ok : Status::Error;
    }

    Status ReadTicket(AppId_t appId, std::vector<uint8_t>& out) {
        const std::string path = AppRegPath(appId);
        DWORD type = 0, size = 0;
        LSTATUS st = RegGetValueA(HKEY_CURRENT_USER, path.c_str(), "AppTicket",
                                   RRF_RT_REG_BINARY, &type, nullptr, &size);
        if (st != ERROR_SUCCESS || size == 0 || size > 1024u * 1024u)
            return LastErrorToStatus();

        out.resize(size);
        st = RegGetValueA(HKEY_CURRENT_USER, path.c_str(), "AppTicket",
                           RRF_RT_REG_BINARY, &type, out.data(), &size);
        if (st != ERROR_SUCCESS || type != REG_BINARY) {
            out.clear();
            return Status::Error;
        }
        out.resize(size);
        return Status::Ok;
    }

    Status WriteTicket(AppId_t appId, const std::vector<uint8_t>& data) {
        const std::string path = AppRegPath(appId);
        HKEY hKey = nullptr;
        DWORD disp = 0;
        LSTATUS st = RegCreateKeyExA(HKEY_CURRENT_USER, path.c_str(), 0, nullptr,
                                      REG_OPTION_NON_VOLATILE, KEY_SET_VALUE,
                                      nullptr, &hKey, &disp);
        if (st != ERROR_SUCCESS) return LastErrorToStatus();

        st = RegSetValueExA(hKey, "AppTicket", 0, REG_BINARY,
                             data.data(), static_cast<DWORD>(data.size()));
        RegCloseKey(hKey);
        LOG_MISC_INFO("CredentialStore: wrote AppTicket AppId={} bytes={}", appId, data.size());
        return (st == ERROR_SUCCESS) ? Status::Ok : Status::Error;
    }

    Status ReadETicket(AppId_t appId, std::vector<uint8_t>& out) {
        const std::string path = AppRegPath(appId);
        DWORD type = 0, size = 0;
        LSTATUS st = RegGetValueA(HKEY_CURRENT_USER, path.c_str(), "ETicket",
                                   RRF_RT_REG_BINARY, &type, nullptr, &size);
        if (st != ERROR_SUCCESS || size == 0 || size > 1024u * 1024u)
            return LastErrorToStatus();

        out.resize(size);
        st = RegGetValueA(HKEY_CURRENT_USER, path.c_str(), "ETicket",
                           RRF_RT_REG_BINARY, &type, out.data(), &size);
        if (st != ERROR_SUCCESS || type != REG_BINARY) {
            out.clear();
            return Status::Error;
        }
        out.resize(size);
        return Status::Ok;
    }

    Status WriteETicket(AppId_t appId, const std::vector<uint8_t>& data) {
        const std::string path = AppRegPath(appId);
        HKEY hKey = nullptr;
        DWORD disp = 0;
        LSTATUS st = RegCreateKeyExA(HKEY_CURRENT_USER, path.c_str(), 0, nullptr,
                                      REG_OPTION_NON_VOLATILE, KEY_SET_VALUE,
                                      nullptr, &hKey, &disp);
        if (st != ERROR_SUCCESS) return LastErrorToStatus();

        st = RegSetValueExA(hKey, "ETicket", 0, REG_BINARY,
                             data.data(), static_cast<DWORD>(data.size()));
        RegCloseKey(hKey);
        LOG_MISC_INFO("CredentialStore: wrote ETicket AppId={} bytes={}", appId, data.size());
        return (st == ERROR_SUCCESS) ? Status::Ok : Status::Error;
    }

    Status GetActiveUser(ActiveUser& out) {
        DWORD accountId = 0, size = sizeof(accountId), type = 0;
        LSTATUS st = RegGetValueA(HKEY_CURRENT_USER,
                                   "Software\\Valve\\Steam\\ActiveProcess",
                                   "ActiveUser",
                                   RRF_RT_REG_DWORD, &type,
                                   &accountId, &size);
        if (st != ERROR_SUCCESS || size != sizeof(accountId) || type != REG_DWORD)
            return Status::NotFound;
        if (accountId == 0) return Status::NotFound;

        out.accountId = accountId;

        DWORD uSize = 0, uType = 0;
        st = RegGetValueA(HKEY_CURRENT_USER,
                           "Software\\Valve\\Steam\\ActiveProcess",
                           "Universe",
                           RRF_RT_REG_SZ, &uType, nullptr, &uSize);
        if (st == ERROR_SUCCESS && uSize > 0) {
            std::wstring uni(uSize / sizeof(wchar_t), L'\0');
            st = RegGetValueW(HKEY_CURRENT_USER,
                               L"Software\\Valve\\Steam\\ActiveProcess",
                               L"Universe",
                               RRF_RT_REG_SZ, &uType, uni.data(), &uSize);
            if (st == ERROR_SUCCESS) {
                while (!uni.empty() && uni.back() == L'\0')
                    uni.pop_back();
                out.universe = uni;
            }
        }
        return Status::Ok;
    }

}
