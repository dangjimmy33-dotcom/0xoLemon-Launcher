// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "hooks/client/IpcMethodLoader.h"
#include "runtime/Logger.h"
#include "runtime/RuntimeHttp.h"
#include "core/entry.h"

#include <toml++/toml.hpp>
#include <filesystem>
#include <fstream>
#include <unordered_map>
#include <mutex>
#include <vector>
#include <array>
#include <cstdlib>

#include <bcrypt.h>
#pragma comment(lib, "bcrypt.lib")

namespace IpcLoader {

    namespace {
        std::mutex g_mtx;
        bool g_loaded = false;

        struct MethodKey {
            std::string iface;
            std::string method;

            bool operator==(const MethodKey& o) const {
                return iface == o.iface && method == o.method;
            }
        };
        struct MethodKeyHash {
            size_t operator()(const MethodKey& k) const {
                return std::hash<std::string>()(k.iface) ^ (std::hash<std::string>()(k.method) << 1);
            }
        };

        std::unordered_map<MethodKey, MethodMeta, MethodKeyHash> g_methods;

        // FNV-1a 32-bit
        constexpr uint32_t kFnvOffset = 2166136261u;
        constexpr uint32_t kFnvPrime = 16777619u;

        constexpr const char* kIPCSubdir = "steamclientipc";

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

        std::string Sha256OfFile(const std::string& path) {
            HANDLE hFile = CreateFileA(path.c_str(), GENERIC_READ,
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

        std::filesystem::path CachePathForSha(const std::string& sha) {
            return std::filesystem::path(SteamInstallPath) / "lumacore" / "pattern" / kIPCSubdir / (sha + ".toml");
        }

        std::string StitchGitflicBlobLines(std::string_view body) {
            constexpr std::string_view kKey = "\"blobLines\"";
            size_t k = body.find(kKey);
            if (k == std::string_view::npos) return {};
            size_t arrStart = body.find('[', k);
            if (arrStart == std::string_view::npos) return {};
            std::string out;
            out.reserve(body.size() / 2);
            size_t pos = arrStart + 1;
            const std::string_view bodyKey = "\"body\"";
            while (pos < body.size()) {
                size_t bk = body.find(bodyKey, pos);
                if (bk == std::string_view::npos) break;
                size_t arrEnd = body.find(']', pos);
                if (arrEnd != std::string_view::npos && bk > arrEnd) break;
                size_t colon = body.find(':', bk + bodyKey.size());
                if (colon == std::string_view::npos) break;
                size_t q1 = body.find('"', colon);
                if (q1 == std::string_view::npos) break;
                std::string line;
                line.reserve(64);
                bool escaped = false;
                size_t p = q1 + 1;
                for (; p < body.size(); ++p) {
                    char c = body[p];
                    if (escaped) {
                        switch (c) {
                            case 'n': line.push_back('\n'); break;
                            case 't': line.push_back('\t'); break;
                            case 'r': line.push_back('\r'); break;
                            case '"': line.push_back('"');  break;
                            case '\\': line.push_back('\\'); break;
                            case '/': line.push_back('/');  break;
                            default:  line.push_back(c);    break;
                        }
                        escaped = false;
                        continue;
                    }
                    if (c == '\\') { escaped = true; continue; }
                    if (c == '"') break;
                    line.push_back(c);
                }
                if (p >= body.size()) break;
                if (!out.empty()) out.push_back('\n');
                out.append(line);
                pos = p + 1;
                if (out.size() > (1u << 20)) break;
            }
            return out;
        }

        void WriteCache(const std::filesystem::path& cachePath, std::string_view body) {
            std::error_code ec;
            std::filesystem::create_directories(cachePath.parent_path(), ec);
            if (ec) return;
            std::ofstream out(cachePath, std::ios::binary | std::ios::trunc);
            if (!out) return;
            out.write(body.data(), static_cast<std::streamsize>(body.size()));
        }

        bool FetchFromNetwork(const std::string& sha, std::string& bodyOut) {
            const std::string ghUrl = "https://raw.githubusercontent.com/KoriaPolis/Steam-Auto-PT/pattern/"
                                      + std::string(kIPCSubdir) + "/" + sha + ".toml";
            const std::string cdnUrl = "https://cdn.jsdelivr.net/gh/KoriaPolis/Steam-Auto-PT@pattern/"
                                       + std::string(kIPCSubdir) + "/" + sha + ".toml";
            const std::string gfUrl = "https://gitflic.ru/api/project/midrags/steam-auto-pt/blob?branch=pattern&file="
                                      + std::string(kIPCSubdir) + "/" + sha + ".toml";

            for (const auto& url : {ghUrl, cdnUrl}) {
                LOG_IPC_DEBUG("IpcLoader: fetching ipc_methods from {}", url);
                auto resp = RuntimeHttp::Get(url);
                if (!resp.networkError && resp.status == 200 && !resp.body.empty()) {
                    bodyOut = std::move(resp.body);
                    return true;
                }
                if (resp.status == 404) {
                    LOG_WARN("IpcLoader: mirror has no such file (HTTP 404): {}", url);
                    break;
                }
            }

            LOG_IPC_DEBUG("IpcLoader: fetching ipc_methods from gitflic");
            auto gfResp = RuntimeHttp::Get(gfUrl);
            if (!gfResp.networkError && gfResp.status == 200 && !gfResp.body.empty()) {
                std::string stitched = StitchGitflicBlobLines(gfResp.body);
                if (!stitched.empty()) {
                    bodyOut = std::move(stitched);
                    return true;
                }
                LOG_WARN("IpcLoader: gitflic stitch failed");
            }

            return false;
        }

        bool TryFetch(const std::string& steamclientPath) {
            std::string sha = Sha256OfFile(steamclientPath);
            if (sha.size() != 64) {
                LOG_WARN("IpcLoader: SHA256 of steamclient failed");
                return false;
            }

            auto cachePath = CachePathForSha(sha);
            if (std::filesystem::exists(cachePath)) {
                LOG_IPC_DEBUG("IpcLoader: using cached ipc_methods at {}", cachePath.string());
                return true;
            }

            std::string body;
            if (!FetchFromNetwork(sha, body)) {
                LOG_WARN("IpcLoader: all network legs failed for sha={}", sha);
                return false;
            }

            WriteCache(cachePath, body);
            LOG_IPC_INFO("IpcLoader: cached ipc_methods to {} ({} bytes)", cachePath.string(), body.size());
            return true;
        }

        bool TryLoad(const std::string& steamclientPath) {
            if (!TryFetch(steamclientPath)) return false;

            std::string sha = Sha256OfFile(steamclientPath);
            auto cachePath = CachePathForSha(sha);
            if (!std::filesystem::exists(cachePath)) return false;

            try {
                auto tbl = toml::parse_file(cachePath.string());
                g_methods.clear();

                for (const auto& [ifaceKey, ifaceNode] : tbl) {
                    auto ifaceTbl = ifaceNode.as_table();
                    if (!ifaceTbl) continue;

                    for (const auto& [methodKey, methodNode] : *ifaceTbl) {
                        auto methodTbl = methodNode.as_table();
                        if (!methodTbl) continue;

                        auto hashNode = (*methodTbl)["funcHash"];
                        if (!hashNode) continue;

                        MethodMeta meta{};
                        if (auto v = (*methodTbl)["funcHash"].value<std::string>()) {
                            meta.funcHash = static_cast<uint32_t>(std::strtoul(v->c_str(), nullptr, 16));
                        }
                        if (auto v = (*methodTbl)["fencepost"].value<std::string>()) {
                            meta.fencepost = static_cast<uint32_t>(std::strtoul(v->c_str(), nullptr, 16));
                        }
                        if (auto v = (*methodTbl)["argc"].value<int64_t>()) {
                            meta.argc = static_cast<uint32_t>(*v);
                        }

                        std::string ifaceName(ifaceKey.str());
                        std::string methodName(methodKey.str());
                        g_methods[{ifaceName, methodName}] = meta;
                        LOG_IPC_DEBUG("IpcLoader: registered {}::{} hash=0x{:08X} fencepost=0x{:08X} argc={}",
                                       ifaceName, methodName, meta.funcHash, meta.fencepost, meta.argc);
                    }
                }

                LOG_IPC_INFO("IpcLoader: loaded {} method(s) from {}", g_methods.size(), cachePath.string());
                return true;

            } catch (const toml::parse_error& e) {
                LOG_WARN("IpcLoader: TOML parse error: {}", e.what());
                return false;
            }
        }
    }

    bool Load(const std::string& steamclientPath) {
        std::scoped_lock lock(g_mtx);
        if (g_loaded) return true;
        g_loaded = TryLoad(steamclientPath);
        return g_loaded;
    }

    bool IsLoaded() {
        std::scoped_lock lock(g_mtx);
        return g_loaded;
    }

    const MethodMeta* Find(std::string_view ifaceName, std::string_view methodName) {
        std::scoped_lock lock(g_mtx);
        auto it = g_methods.find(MethodKey{std::string(ifaceName), std::string(methodName)});
        return it != g_methods.end() ? &it->second : nullptr;
    }

    uint32_t HashInterfaceName(std::string_view name) {
        uint32_t h = kFnvOffset;
        for (char c : name) {
            h ^= static_cast<uint8_t>(c);
            h *= kFnvPrime;
        }
        return h;
    }

    uint32_t HashMethodName(std::string_view name) {
        uint32_t h = kFnvOffset;
        for (char c : name) {
            h ^= static_cast<uint8_t>(c);
            h *= kFnvPrime;
        }
        return h;
    }

}
