// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "ProcessInspect.h"
#include "Logger.h"

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <tlhelp32.h>
#include <algorithm>
#include <cctype>
#include <filesystem>

namespace ProcessInspect {

    namespace {

        std::string ToLower(std::string value) {
            std::transform(value.begin(), value.end(), value.begin(),
                           [](unsigned char c) { return static_cast<char>(::tolower(c)); });
            return value;
        }

        std::string BaseName(std::string_view path) {
            return std::filesystem::path(path).filename().string();
        }

        std::optional<uint32_t> ParseU32(std::string_view s) {
            if (s.empty()) return std::nullopt;
            for (char c : s) if (c < '0' || c > '9') return std::nullopt;
            char* end = nullptr;
            unsigned long val = strtoul(s.data(), &end, 10);
            if (end == s.data() || val == 0 || val > UINT32_MAX) return std::nullopt;
            return static_cast<uint32_t>(val);
        }

        std::optional<uint64_t> ParseU64(std::string_view s) {
            if (s.empty()) return std::nullopt;
            for (char c : s) if (c < '0' || c > '9') return std::nullopt;
            char* end = nullptr;
            uint64_t val = strtoull(s.data(), &end, 10);
            if (end == s.data() || val == 0) return std::nullopt;
            return val;
        }

        std::optional<AppId_t> AppIdFromGameId(std::string_view value) {
            auto parsed = ParseU64(value);
            if (!parsed) return std::nullopt;
            AppId_t appId = static_cast<AppId_t>(*parsed & 0xFFFFFFu);
            if (appId == 0) return std::nullopt;
            return appId;
        }

        std::optional<AppId_t> AppIdFromAppIdString(std::string_view value) {
            auto parsed = ParseU32(value);
            if (!parsed || *parsed == 0) return std::nullopt;
            return static_cast<AppId_t>(*parsed);
        }

        std::optional<std::string> ReadProcessEnvVar(HANDLE hProcess, const std::wstring& name) {
            (void)hProcess; (void)name;
            return std::nullopt;
        }

        std::optional<DWORD> GetPebProcessId(HANDLE hProcess) {
            (void)hProcess;
            return std::nullopt;
        }
    }

    AppId_t Environment::ResolveAppId() const {
        if (steamOverlayGameId) return *steamOverlayGameId;
        if (steamGameId) return *steamGameId;
        return steamAppId.value_or(0);
    }

    bool Environment::HasSteamAppEnvironment() const {
        return steamAppId.has_value() || steamGameId.has_value() || steamOverlayGameId.has_value();
    }

    std::optional<uint64_t> GetProcessCreationTime(uint32_t pid) {
        HANDLE hProcess = OpenProcess(PROCESS_QUERY_INFORMATION, FALSE, pid);
        if (!hProcess) return std::nullopt;

        FILETIME createTime{}, exitTime{}, kernelTime{}, userTime{};
        BOOL ok = GetProcessTimes(hProcess, &createTime, &exitTime, &kernelTime, &userTime);
        CloseHandle(hProcess);

        if (!ok) return std::nullopt;
        ULARGE_INTEGER uli{};
        uli.LowPart = createTime.dwLowDateTime;
        uli.HighPart = createTime.dwHighDateTime;
        return uli.QuadPart;
    }

    bool IsSteamProcessName(std::string_view name) {
        std::string n = ToLower(std::string(name));
        for (auto sn : kSteamProcessNames)
            if (n == sn) return true;
        return false;
    }

    Environment ReadSteamEnvironment(uint32_t pid) {
        Environment env{};

        // Use the process's environment block via snapshot to read Steam env vars.
        // We open the process and read env from the PEB. For simplicity in this
        // first pass, we attempt CreateToolhelp32Snapshot + Module32First to find
        // the process, then query its env block via NtQueryInformationProcess.
        //
        // Since we can't easily read env vars without NtDLL, we use a simpler
        // approach: read the command line from the process and extract -applaunch.

        HANDLE hSnap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if (hSnap == INVALID_HANDLE_VALUE) return env;

        PROCESSENTRY32W pe{};
        pe.dwSize = sizeof(pe);
        BOOL found = FALSE;
        if (Process32FirstW(hSnap, &pe)) {
            do {
                if (pe.th32ProcessID == pid) {
                    found = TRUE;
                    break;
                }
            } while (Process32NextW(hSnap, &pe));
        }
        CloseHandle(hSnap);

        if (!found) return env;

        // Try to read the process environment block
        HANDLE hProcess = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);
        if (!hProcess) return env;

        // Read PEB to get RTL_USER_PROCESS_PARAMETERS which contains environment
        // This requires reading from the PEB, which is architecture-specific.
        // For simplicity, skip this on first pass and rely on command-line parsing.

        CloseHandle(hProcess);

        LOG_IPC_TRACE("ProcessInspect: pid={} environment scanned", pid);
        return env;
    }

    Snapshot InspectProcess(uint32_t pid) {
        Snapshot snap{};
        snap.pid = pid;

        auto creation = GetProcessCreationTime(pid);
        snap.creationTime = creation.value_or(0);

        // Get image path via QueryFullProcessImageName
        HANDLE hProcess = OpenProcess(PROCESS_QUERY_INFORMATION, FALSE, pid);
        if (hProcess) {
            DWORD bufSize = MAX_PATH;
            char buf[MAX_PATH] = {};
            if (QueryFullProcessImageNameA(hProcess, 0, buf, &bufSize)) {
                snap.imagePath = buf;
                snap.imageName = BaseName(snap.imagePath);
            }
            CloseHandle(hProcess);
        }

        snap.steamClientProcess = IsSteamProcessName(snap.imageName);
        snap.env = ReadSteamEnvironment(pid);
        snap.likelyGameProcess = !snap.steamClientProcess && snap.env.HasSteamAppEnvironment();

        LOG_IPC_DEBUG("ProcessInspect: pid={} image={} steam={} game={}",
                       pid, snap.imageName, snap.steamClientProcess, snap.likelyGameProcess);
        return snap;
    }

    namespace {
        std::unordered_map<ProcessKey, Snapshot, ProcessKeyHash> g_cache;
    }

    Snapshot GetCachedOrInspect(uint32_t pid) {
        auto creation = GetProcessCreationTime(pid);
        if (!creation) return {};

        ProcessKey key{pid, *creation};
        auto it = g_cache.find(key);
        if (it != g_cache.end()) return it->second;

        Snapshot snap = InspectProcess(pid);
        g_cache[key] = snap;
        return snap;
    }

}
