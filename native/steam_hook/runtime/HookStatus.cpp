// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "HookStatus.h"

#include "runtime/Logger.h"
#include "core/entry.h"

#include <windows.h>

#include <cstdio>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <mutex>
#include <string>
#include <string_view>
#include <vector>

namespace HookStatus {

    namespace {

        std::mutex g_mu;

        std::string g_buildId;
        std::string g_steamExePath;
        std::string g_steamclientPath;
        std::string g_steamuiPath;
        std::string g_diversionPath;
        std::string g_steamclientFileSha;
        std::string g_steamuiFileSha;
        std::string g_diversionFileSha;
        std::string g_steamclientSha;
        std::string g_steamuiSha;
        std::string g_loader;
        std::string g_hookTarget;
        std::string g_hookModule;
        std::string g_mappedLoaders;
        bool        g_package0Captured = false;
        bool        g_package0Seeded = false;
        bool        g_startupInjectionDone = false;
        bool        g_licenseRefreshDone = false;
        std::string g_startupPhase = "boot";
        std::string g_startupRefreshState = "idle";
        std::string g_steamLoginPhase = "init";
        bool        g_startupSafe = false;
        std::string g_packageMutationDeferredReason = "not_evaluated";
        bool        g_diversionValidated = false;
        std::string g_diversionReason;
        bool        g_diversionFileReady = false;
        bool        g_diversionLoadReady = false;
        std::string g_diversionStrategy;
        std::string g_diversionLastError;
        std::string g_steamUiAttachState;
        int         g_steamUiAttachAttempts = 0;
        bool        g_activeFallbackUsed = false;
        bool        g_steamclientToml = false;
        bool        g_steamuiToml     = false;
        std::uint64_t            g_installed = 0;
        std::vector<std::string> g_missed;
        bool        g_initDone        = false;

        // Conservative escaper for JSON string literals. The values we emit are
        // ASCII function names, hex SHAs, and decimal build ids, so anything
        // outside printable ASCII falls through to \uXXXX.
        std::string JsonEscape(std::string_view s) {
            std::string out;
            out.reserve(s.size() + 2);
            for (char ch : s) {
                unsigned char c = static_cast<unsigned char>(ch);
                switch (c) {
                    case '"':  out += "\\\""; break;
                    case '\\': out += "\\\\"; break;
                    case '\b': out += "\\b";  break;
                    case '\f': out += "\\f";  break;
                    case '\n': out += "\\n";  break;
                    case '\r': out += "\\r";  break;
                    case '\t': out += "\\t";  break;
                    default:
                        if (c < 0x20 || c > 0x7E) {
                            char buf[8];
                            std::snprintf(buf, sizeof(buf), "\\u%04X", c);
                            out += buf;
                        } else {
                            out += static_cast<char>(c);
                        }
                        break;
                }
            }
            return out;
        }

        // Caller already owns g_mu.
        std::string SerializeLocked() {
            std::string out;
            out.reserve(256 + g_missed.size() * 32);
            out += "{\n";
            out += "  \"build_id\": \"";
            out += JsonEscape(g_buildId);
            out += "\",\n";
            out += "  \"steam_exe_path\": \"";
            out += JsonEscape(g_steamExePath);
            out += "\",\n";
            out += "  \"steamclient_path\": \"";
            out += JsonEscape(g_steamclientPath);
            out += "\",\n";
            out += "  \"steamui_path\": \"";
            out += JsonEscape(g_steamuiPath);
            out += "\",\n";
            out += "  \"diversion_path\": \"";
            out += JsonEscape(g_diversionPath);
            out += "\",\n";
            out += "  \"steamclient_file_sha\": \"";
            out += JsonEscape(g_steamclientFileSha);
            out += "\",\n";
            out += "  \"steamui_file_sha\": \"";
            out += JsonEscape(g_steamuiFileSha);
            out += "\",\n";
            out += "  \"diversion_file_sha\": \"";
            out += JsonEscape(g_diversionFileSha);
            out += "\",\n";
            out += "  \"toml_found\": {\n";
            out += "    \"steamclient\": ";
            out += g_steamclientToml ? "true" : "false";
            out += ",\n";
            out += "    \"steamui\": ";
            out += g_steamuiToml ? "true" : "false";
            out += "\n  },\n";
            out += "  \"hooks_installed\": ";
            out += std::to_string(g_installed);
            out += ",\n";
            out += "  \"hooks_missed\": [";
            for (size_t i = 0; i < g_missed.size(); ++i) {
                if (i) out += ", ";
                out += "\"";
                out += JsonEscape(g_missed[i]);
                out += "\"";
            }
            out += "],\n";
            out += "  \"steamclient_sha\": \"";
            out += JsonEscape(g_steamclientSha);
            out += "\",\n";
            out += "  \"steamui_sha\": \"";
            out += JsonEscape(g_steamuiSha);
            out += "\",\n";
            out += "  \"loader\": \"";
            out += JsonEscape(g_loader);
            out += "\",\n";
            out += "  \"hook_target\": \"";
            out += JsonEscape(g_hookTarget);
            out += "\",\n";
            out += "  \"hook_module\": \"";
            out += JsonEscape(g_hookModule);
            out += "\",\n";
            out += "  \"mapped_loaders\": \"";
            out += JsonEscape(g_mappedLoaders);
            out += "\",\n";
            out += "  \"package0_captured\": ";
            out += g_package0Captured ? "true" : "false";
            out += ",\n";
            out += "  \"package0_seeded\": ";
            out += g_package0Seeded ? "true" : "false";
            out += ",\n";
            out += "  \"startup_injection_done\": ";
            out += g_startupInjectionDone ? "true" : "false";
            out += ",\n";
            out += "  \"license_refresh_done\": ";
            out += g_licenseRefreshDone ? "true" : "false";
            out += ",\n";
            out += "  \"startup_phase\": \"";
            out += JsonEscape(g_startupPhase);
            out += "\",\n";
            out += "  \"startup_refresh_state\": \"";
            out += JsonEscape(g_startupRefreshState);
            out += "\",\n";
            out += "  \"steam_login_phase\": \"";
            out += JsonEscape(g_steamLoginPhase);
            out += "\",\n";
            out += "  \"startup_safe\": ";
            out += g_startupSafe ? "true" : "false";
            out += ",\n";
            out += "  \"package_mutation_deferred_reason\": \"";
            out += JsonEscape(g_packageMutationDeferredReason);
            out += "\",\n";
            out += "  \"diversion_validated\": ";
            out += g_diversionValidated ? "true" : "false";
            out += ",\n";
            out += "  \"diversion_reason\": \"";
            out += JsonEscape(g_diversionReason);
            out += "\",\n";
            out += "  \"diversion_file_ready\": ";
            out += g_diversionFileReady ? "true" : "false";
            out += ",\n";
            out += "  \"diversion_load_ready\": ";
            out += g_diversionLoadReady ? "true" : "false";
            out += ",\n";
            out += "  \"diversion_strategy\": \"";
            out += JsonEscape(g_diversionStrategy);
            out += "\",\n";
            out += "  \"diversion_last_error\": \"";
            out += JsonEscape(g_diversionLastError);
            out += "\",\n";
            out += "  \"steamui_attach_state\": \"";
            out += JsonEscape(g_steamUiAttachState);
            out += "\",\n";
            out += "  \"steamui_attach_attempts\": ";
            out += std::to_string(g_steamUiAttachAttempts);
            out += ",\n";
            out += "  \"active_fallback_used\": ";
            out += g_activeFallbackUsed ? "true" : "false";
            out += "\n";
            out += "}\n";
            return out;
        }

        bool WriteBodyAtomic(const std::string& body) {
            if (!SteamInstallPath[0]) {
                LOG_WARN("HookStatus: SteamInstallPath unset, skipping write");
                return false;
            }
            std::filesystem::path dir = std::filesystem::path(SteamInstallPath) / "0xocore";
            std::error_code ec;
            std::filesystem::create_directories(dir, ec);
            if (ec) {
                LOG_WARN("HookStatus: create_directories failed: {}", ec.message());
                return false;
            }

            std::filesystem::path target = dir / "status.json";
            std::filesystem::path tmp    = target;
            tmp += ".tmp";

            std::string narrowTmp    = tmp.string();
            std::string narrowTarget = target.string();

            {
                std::ofstream f(tmp, std::ios::binary | std::ios::trunc);
                if (!f) {
                    LOG_WARN("HookStatus: open tmp failed for {}", narrowTarget);
                    DeleteFileA(narrowTmp.c_str());
                    return false;
                }
                f.write(body.data(), static_cast<std::streamsize>(body.size()));
                f.flush();
                if (!f) {
                    LOG_WARN("HookStatus: write tmp failed for {}", narrowTarget);
                    f.close();
                    DeleteFileA(narrowTmp.c_str());
                    return false;
                }
            }

            if (!MoveFileExA(narrowTmp.c_str(), narrowTarget.c_str(),
                             MOVEFILE_REPLACE_EXISTING)) {
                DWORD err = GetLastError();
                LOG_WARN("HookStatus: MoveFileExA failed err={} for {}",
                         err, narrowTarget);
                DeleteFileA(narrowTmp.c_str());
                return false;
            }
            return true;
        }

        // Called from any mutator while holding g_mu. Re-publishes the file
        // only after the first explicit WriteToDisk has flipped g_initDone.
        void MaybeRepublishLocked() {
            if (!g_initDone) return;
            std::string body = SerializeLocked();
            (void)WriteBodyAtomic(body);
        }

        bool CsvHasToken(std::string_view csv, std::string_view token) {
            if (token.empty()) return true;
            size_t start = 0;
            while (start <= csv.size()) {
                const size_t comma = csv.find(',', start);
                const size_t end = comma == std::string_view::npos ? csv.size() : comma;
                if (csv.substr(start, end - start) == token)
                    return true;
                if (comma == std::string_view::npos)
                    break;
                start = comma + 1;
            }
            return false;
        }

        void MergeCsvTokens(std::string& target, std::string_view incoming) {
            size_t start = 0;
            while (start <= incoming.size()) {
                const size_t comma = incoming.find(',', start);
                const size_t end = comma == std::string_view::npos ? incoming.size() : comma;
                const std::string_view token = incoming.substr(start, end - start);
                if (!CsvHasToken(target, token)) {
                    if (!target.empty()) target += ',';
                    target.append(token.data(), token.size());
                }
                if (comma == std::string_view::npos)
                    break;
                start = comma + 1;
            }
        }

    }  // namespace

    void SetBuildId(std::string buildId) {
        std::lock_guard<std::mutex> lk(g_mu);
        g_buildId = std::move(buildId);
    }

    void SetBinarySnapshot(std::string steamExePath,
                           std::string steamclientPath,
                           std::string steamuiPath,
                           std::string diversionPath,
                           std::string steamclientFileSha,
                           std::string steamuiFileSha,
                           std::string diversionFileSha) {
        std::lock_guard<std::mutex> lk(g_mu);
        g_steamExePath = std::move(steamExePath);
        g_steamclientPath = std::move(steamclientPath);
        g_steamuiPath = std::move(steamuiPath);
        g_diversionPath = std::move(diversionPath);
        g_steamclientFileSha = std::move(steamclientFileSha);
        g_steamuiFileSha = std::move(steamuiFileSha);
        g_diversionFileSha = std::move(diversionFileSha);
        MaybeRepublishLocked();
    }

    void SetLoaderState(std::string loader, std::string hookTarget, std::string hookModule) {
        std::lock_guard<std::mutex> lk(g_mu);
        g_loader = std::move(loader);
        g_hookTarget = std::move(hookTarget);
        g_hookModule = std::move(hookModule);
        MaybeRepublishLocked();
    }

    void SetPackageState(bool package0Captured, bool package0Seeded,
                         bool startupInjectionDone, bool licenseRefreshDone) {
        std::lock_guard<std::mutex> lk(g_mu);
        g_package0Captured = g_package0Captured || package0Captured;
        g_package0Seeded = g_package0Seeded || package0Seeded;
        g_startupInjectionDone = g_startupInjectionDone || startupInjectionDone;
        g_licenseRefreshDone = g_licenseRefreshDone || licenseRefreshDone;
        MaybeRepublishLocked();
    }

    void SetStartupPhase(std::string phase) {
        std::lock_guard<std::mutex> lk(g_mu);
        g_startupPhase = std::move(phase);
        MaybeRepublishLocked();
    }

    void SetStartupRefreshState(std::string state) {
        std::lock_guard<std::mutex> lk(g_mu);
        g_startupRefreshState = std::move(state);
        MaybeRepublishLocked();
    }

    void SetStartupSafety(std::string phase, bool safe, std::string deferredReason) {
        std::lock_guard<std::mutex> lk(g_mu);
        g_steamLoginPhase = std::move(phase);
        g_startupSafe = safe;
        g_packageMutationDeferredReason = std::move(deferredReason);
        MaybeRepublishLocked();
    }

    void SetMappedLoaders(std::string mappedLoaders) {
        std::lock_guard<std::mutex> lk(g_mu);
        MergeCsvTokens(g_mappedLoaders, mappedLoaders);
        MaybeRepublishLocked();
    }

    void SetDiversionState(bool validated, std::string reason) {
        std::lock_guard<std::mutex> lk(g_mu);
        g_diversionValidated = validated;
        g_diversionReason = std::move(reason);
        MaybeRepublishLocked();
    }

    void SetDiversionDetails(bool fileReady, bool loadReady,
                             std::string strategy, std::string lastError) {
        std::lock_guard<std::mutex> lk(g_mu);
        g_diversionFileReady = fileReady;
        g_diversionLoadReady = loadReady;
        g_diversionStrategy = std::move(strategy);
        g_diversionLastError = std::move(lastError);
        MaybeRepublishLocked();
    }

    void SetSteamUiAttachState(std::string state, int attempts, bool activeFallbackUsed) {
        std::lock_guard<std::mutex> lk(g_mu);
        g_steamUiAttachState = std::move(state);
        g_steamUiAttachAttempts = attempts;
        g_activeFallbackUsed = activeFallbackUsed;
        MaybeRepublishLocked();
    }

    void SetTomlAvailability(std::string_view moduleName, bool found) {
        std::lock_guard<std::mutex> lk(g_mu);
        if (moduleName == "steamclient") {
            g_steamclientToml = found;
        } else if (moduleName == "steamui") {
            g_steamuiToml = found;
        } else {
            LOG_WARN("HookStatus: unknown module '{}' in SetTomlAvailability",
                     std::string(moduleName));
            return;
        }
    }

    void SetShas(std::string steamclientSha, std::string steamuiSha) {
        std::lock_guard<std::mutex> lk(g_mu);
        g_steamclientSha = std::move(steamclientSha);
        g_steamuiSha     = std::move(steamuiSha);
    }

    void RecordInstalled() {
        std::lock_guard<std::mutex> lk(g_mu);
        ++g_installed;
    }

    void RecordMissed(std::string hookName) {
        if (hookName.empty()) return;
        std::lock_guard<std::mutex> lk(g_mu);
        g_missed.push_back(std::move(hookName));
    }

    void WriteToDisk() {
        std::string body;
        {
            std::lock_guard<std::mutex> lk(g_mu);
            body = SerializeLocked();
            g_initDone = true;
        }
        try {
            (void)WriteBodyAtomic(body);
        } catch (const std::exception& e) {
            LOG_WARN("HookStatus: write threw '{}'", e.what());
        } catch (...) {
            LOG_WARN("HookStatus: write threw unknown");
        }
    }

}  // namespace HookStatus
