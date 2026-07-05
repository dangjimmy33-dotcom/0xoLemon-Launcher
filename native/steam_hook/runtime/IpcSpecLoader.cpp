// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "runtime/IpcSpecLoader.h"

#include "core/entry.h"
#include "runtime/Logger.h"
#include "config/Settings.h"

#include <windows.h>
#include <bcrypt.h>
#include <winhttp.h>

#include <toml++/toml.hpp>

#include <algorithm>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <mutex>
#include <sstream>
#include <string>
#include <string_view>
#include <vector>

#pragma comment(lib, "bcrypt.lib")
#pragma comment(lib, "winhttp.lib")

namespace IpcSpecLoader {

    namespace {

        constexpr DWORD       kHttpTimeoutMs = 10'000;
        constexpr std::size_t kMaxBodyBytes  = 1u << 20;

        // ── loaded state ────────────────────────────────────────────────────

        std::once_flag              g_loadFlag;
        bool                        g_loaded    = false;
        std::vector<InterfaceSpec>  g_interfaces;
        std::mutex                  g_mutex;

        // ── small helpers ───────────────────────────────────────────────────

        std::string ToHexLower(const std::uint8_t* data, std::size_t len) {
            static const char kDigits[] = "0123456789abcdef";
            std::string out;
            out.resize(len * 2);
            for (std::size_t i = 0; i < len; ++i) {
                out[2 * i + 0] = kDigits[(data[i] >> 4) & 0xF];
                out[2 * i + 1] = kDigits[data[i] & 0xF];
            }
            return out;
        }

        std::string Sha256OfFile(const std::wstring& path) {
            HANDLE hFile = CreateFileW(path.c_str(), GENERIC_READ,
                                       FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                                       nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
            if (hFile == INVALID_HANDLE_VALUE) return {};

            BCRYPT_ALG_HANDLE  hAlg  = nullptr;
            BCRYPT_HASH_HANDLE hHash = nullptr;
            std::array<std::uint8_t, 32> digest{};
            std::vector<std::uint8_t> buf(1u << 20);
            std::string out;

            do {
                if (BCryptOpenAlgorithmProvider(&hAlg, BCRYPT_SHA256_ALGORITHM, nullptr, 0) != 0) break;
                if (BCryptCreateHash(hAlg, &hHash, nullptr, 0, nullptr, 0, 0) != 0) break;

                bool ok = true;
                for (;;) {
                    DWORD got = 0;
                    if (!ReadFile(hFile, buf.data(), static_cast<DWORD>(buf.size()), &got, nullptr)) {
                        ok = false; break;
                    }
                    if (got == 0) break;
                    if (BCryptHashData(hHash, buf.data(), got, 0) != 0) { ok = false; break; }
                }
                if (!ok) break;
                if (BCryptFinishHash(hHash, digest.data(), static_cast<ULONG>(digest.size()), 0) != 0) break;

                out = ToHexLower(digest.data(), digest.size());
            } while (false);

            if (hHash) BCryptDestroyHash(hHash);
            if (hAlg)  BCryptCloseAlgorithmProvider(hAlg, 0);
            CloseHandle(hFile);
            return out;
        }

        std::wstring ModuleDiskPathW() {
            wchar_t buf[MAX_PATH] = {};
            DWORD n = GetModuleFileNameW(diversion_hModule, buf, MAX_PATH);
            if (n == 0 || n == MAX_PATH) return {};
            return std::wstring(buf, n);
        }

        // ── cache path ──────────────────────────────────────────────────────

        std::filesystem::path CacheDir() {
            return std::filesystem::path(SteamInstallPath) / "0xocore" / "pattern";
        }

        std::filesystem::path CachePath(const std::string& sha) {
            return CacheDir() / (sha + ".toml");
        }

        // ── HTTP GET (minimal, single-leg) ──────────────────────────────────

        struct HttpResult {
            bool        ok    = false;
            int         status = 0;
            std::string body;
        };

        HttpResult HttpGet(const wchar_t* host, const wchar_t* path) {
            HttpResult r{};
            HINTERNET hSession = WinHttpOpen(L"LumaCore-IpcSpecLoader/1.0",
                                             WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                                             WINHTTP_NO_PROXY_NAME, WINHTTP_NO_PROXY_BYPASS, 0);
            if (!hSession) return r;

            WinHttpSetTimeouts(hSession,
                               static_cast<int>(kHttpTimeoutMs),
                               static_cast<int>(kHttpTimeoutMs),
                               static_cast<int>(kHttpTimeoutMs),
                               static_cast<int>(kHttpTimeoutMs));

            HINTERNET hConn = WinHttpConnect(hSession, host, INTERNET_DEFAULT_HTTPS_PORT, 0);
            if (!hConn) { WinHttpCloseHandle(hSession); return r; }

            HINTERNET hReq = WinHttpOpenRequest(hConn, L"GET", path, nullptr,
                                                WINHTTP_NO_REFERER,
                                                WINHTTP_DEFAULT_ACCEPT_TYPES,
                                                WINHTTP_FLAG_SECURE);
            if (!hReq) { WinHttpCloseHandle(hConn); WinHttpCloseHandle(hSession); return r; }

            BOOL ok = WinHttpSendRequest(hReq, WINHTTP_NO_ADDITIONAL_HEADERS, 0,
                                         WINHTTP_NO_REQUEST_DATA, 0, 0, 0);
            if (ok) ok = WinHttpReceiveResponse(hReq, nullptr);
            if (!ok) {
                WinHttpCloseHandle(hReq);
                WinHttpCloseHandle(hConn);
                WinHttpCloseHandle(hSession);
                return r;
            }

            DWORD status = 0;
            DWORD szStatus = sizeof(status);
            WinHttpQueryHeaders(hReq, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                                WINHTTP_HEADER_NAME_BY_INDEX, &status, &szStatus,
                                WINHTTP_NO_HEADER_INDEX);
            r.status = static_cast<int>(status);

            if (r.status == 200) {
                r.body.reserve(64 * 1024);
                std::array<char, 16 * 1024> buf{};
                for (;;) {
                    DWORD avail = 0;
                    if (!WinHttpQueryDataAvailable(hReq, &avail)) break;
                    if (avail == 0) break;
                    while (avail > 0) {
                        DWORD want = std::min<DWORD>(avail, static_cast<DWORD>(buf.size()));
                        DWORD got = 0;
                        if (!WinHttpReadData(hReq, buf.data(), want, &got)) break;
                        if (got == 0) { avail = 0; break; }
                        if (r.body.size() + got > kMaxBodyBytes) { avail = 0; break; }
                        r.body.append(buf.data(), got);
                        avail -= got;
                    }
                }
                r.ok = !r.body.empty();
            }

            WinHttpCloseHandle(hReq);
            WinHttpCloseHandle(hConn);
            WinHttpCloseHandle(hSession);
            return r;
        }

        // ── TOML parsing ────────────────────────────────────────────────────

        bool ParseHex32(std::string_view s, std::uint32_t& out) {
            if (s.empty()) return false;
            std::string trimmed(s);
            if (trimmed.size() >= 2 && trimmed[0] == '0' &&
                (trimmed[1] == 'x' || trimmed[1] == 'X')) {
                trimmed.erase(0, 2);
            }
            if (trimmed.empty()) return false;
            try {
                std::size_t pos = 0;
                unsigned long v = std::stoul(trimmed, &pos, 16);
                if (pos != trimmed.size()) return false;
                out = static_cast<std::uint32_t>(v);
                return true;
            } catch (...) { return false; }
        }

        bool ParseTomlBody(std::string_view body,
                           std::vector<InterfaceSpec>& out)
        {
            out.clear();
            try {
                auto tbl = toml::parse(body);
                for (const auto& [ifaceName, ifaceNode] : tbl) {
                    auto ifaceTable = ifaceNode.as_table();
                    if (!ifaceTable) continue;

                    InterfaceSpec spec;
                    spec.name = ifaceName;

                    auto idNode = (*ifaceTable)["interface_id"].value<int64_t>();
                    if (!idNode) continue;
                    spec.interfaceId = static_cast<uint32_t>(*idNode);

                    for (const auto& [key, val] : *ifaceTable) {
                        if (key == "interface_id") continue;
                        auto methodTable = val.as_table();
                        if (!methodTable) continue;

                        MethodSpec ms;
                        ms.name = key;

                        auto fh = (*methodTable)["funcHash"].value<std::string>();
                        if (!fh || !ParseHex32(*fh, ms.funcHash)) continue;

                        auto fp = (*methodTable)["fencepost"].value<std::string>();
                        if (fp) ParseHex32(*fp, ms.fencepost);

                        auto ac = (*methodTable)["argc"].value<int64_t>();
                        if (ac) ms.argc = static_cast<uint32_t>(*ac);

                        spec.methods.push_back(std::move(ms));
                    }

                    if (!spec.methods.empty())
                        out.push_back(std::move(spec));
                }
            } catch (const toml::parse_error& e) {
                LOG_MISC_WARN("IpcSpecLoader: TOML parse error: {} line {}",
                              e.description(), e.source().begin.line);
                return false;
            } catch (...) {
                LOG_MISC_WARN("IpcSpecLoader: TOML parse unknown error");
                return false;
            }
            return !out.empty();
        }

        // ── atomic cache write ──────────────────────────────────────────────

        bool WriteCache(const std::string& sha, std::string_view body) {
            auto dir  = CacheDir();
            auto path = CachePath(sha);
            std::error_code ec;
            std::filesystem::create_directories(dir, ec);
            if (ec) return false;

            auto tmpPath = path; tmpPath += ".tmp";
            {
                std::ofstream f(tmpPath, std::ios::binary | std::ios::trunc);
                if (!f) return false;
                f.write(body.data(), static_cast<std::streamsize>(body.size()));
                if (!f) return false;
            }
            if (!MoveFileExA(tmpPath.string().c_str(), path.string().c_str(),
                             MOVEFILE_REPLACE_EXISTING)) {
                std::filesystem::remove(tmpPath, ec);
                return false;
            }
            return true;
        }

        // ── cache read ──────────────────────────────────────────────────────

        bool ReadCache(const std::string& sha, std::vector<InterfaceSpec>& out) {
            auto path = CachePath(sha);
            std::error_code ec;
            if (!std::filesystem::exists(path, ec)) return false;
            std::ifstream f(path, std::ios::binary);
            if (!f) return false;
            std::ostringstream oss;
            oss << f.rdbuf();
            std::string body = oss.str();
            if (body.size() > kMaxBodyBytes) return false;
            return ParseTomlBody(body, out);
        }

    } // anonymous namespace

    // ── public surface ──────────────────────────────────────────────────────

    void Load() {
        std::call_once(g_loadFlag, [] {
            std::wstring wpath = ModuleDiskPathW();
            if (wpath.empty()) {
                LOG_MISC_WARN("IpcSpecLoader: cannot resolve steamclient path");
                return;
            }

            std::string sha = Sha256OfFile(wpath);
            if (sha.size() != 64) {
                LOG_MISC_DEBUG("IpcSpecLoader: SHA256 failed for steamclient");
                return;
            }

            std::vector<InterfaceSpec> parsed;

            // Cache-first
            if (ReadCache(sha, parsed)) {
                LOG_MISC_DEBUG("IpcSpecLoader: cache hit sha={} interfaces={}",
                               sha, static_cast<unsigned>(parsed.size()));
                std::lock_guard<std::mutex> lk(g_mutex);
                g_interfaces = std::move(parsed);
                g_loaded = true;
                return;
            }

            // Network fetch: raw.githubusercontent.com
            std::string urlPath = "/KoriaPolis/Steam-Auto-PT/pattern/steamclientipc/";
            urlPath += sha;
            urlPath += ".toml";

            std::wstring wUrlPath(urlPath.begin(), urlPath.end());
            HttpResult hr = HttpGet(L"raw.githubusercontent.com", wUrlPath.c_str());

            if (!hr.ok || hr.status != 200) {
                LOG_MISC_DEBUG("IpcSpecLoader: HTTP {} for ipc/steamclient/{}",
                               hr.status == 0 ? "failed" : std::to_string(hr.status), sha);
                return;
            }

            if (!ParseTomlBody(hr.body, parsed)) {
                LOG_MISC_WARN("IpcSpecLoader: TOML parse failed for ipc/steamclient/{}",
                              sha);
                return;
            }

            WriteCache(sha, hr.body);

            std::lock_guard<std::mutex> lk(g_mutex);
            g_interfaces = std::move(parsed);
            g_loaded = true;

            LOG_MISC_DEBUG("IpcSpecLoader: network hit sha={} interfaces={}",
                           sha, static_cast<unsigned>(g_interfaces.size()));
        });
    }

    std::optional<uint32_t> ResolveHash(const char* qualifiedName) {
        if (!qualifiedName || !g_loaded) return std::nullopt;

        // qualifiedName format: "InterfaceName::MethodName"
        // e.g. "IClientUser::GetSteamID"
        const char* sep = std::strstr(qualifiedName, "::");
        if (!sep) return std::nullopt;

        std::string_view ifaceName(qualifiedName,
                                   static_cast<std::size_t>(sep - qualifiedName));
        std::string_view methodName(sep + 2);

        std::lock_guard<std::mutex> lk(g_mutex);

        // Binary search on interface name (sorted vector)
        auto it = std::lower_bound(g_interfaces.begin(), g_interfaces.end(), ifaceName,
            [](const InterfaceSpec& spec, std::string_view name) {
                return spec.name < name;
            });

        if (it == g_interfaces.end() || it->name != ifaceName)
            return std::nullopt;

        // Linear search on methods (small N, typically <20)
        for (const auto& m : it->methods) {
            if (m.name == methodName)
                return m.funcHash;
        }

        return std::nullopt;
    }

    bool IsLoaded() {
        return g_loaded;
    }

    void Reset() {
        std::lock_guard<std::mutex> lk(g_mutex);
        g_interfaces.clear();
        g_loaded = false;
        // Reset std::once_flag so Load() can run again
        // Not possible with std::once_flag; user must restart process.
        // This is acceptable since Detach only runs on process shutdown.
    }

} // namespace IpcSpecLoader
