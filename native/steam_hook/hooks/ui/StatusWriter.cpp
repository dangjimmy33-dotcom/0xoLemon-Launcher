// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "hooks/ui/StatusWriter.h"

#include "core/entry.h"
#include "runtime/Logger.h"

#include <chrono>
#include <filesystem>
#include <fstream>
#include <mutex>
#include <set>
#include <string>
#include <unordered_map>

namespace StatusWriter {

    namespace {
        struct State {
            std::string buildId;
            std::unordered_map<std::string, std::string> shas;       // subdir -> sha
            std::unordered_map<std::string, bool>        tomlFound;  // subdir -> found?
            std::set<std::string> hits;
            std::set<std::string> misses;
        };

        std::mutex g_mu;
        State      g_state;

        // Escape just enough for JSON: backslash, quote, and control chars
        // below 0x20. Keys we emit are ASCII so no UTF-8 work needed.
        std::string EscapeJson(const std::string& in) {
            std::string out;
            out.reserve(in.size() + 2);
            for (char c : in) {
                switch (c) {
                    case '"':  out += "\\\""; break;
                    case '\\': out += "\\\\"; break;
                    case '\b': out += "\\b";  break;
                    case '\f': out += "\\f";  break;
                    case '\n': out += "\\n";  break;
                    case '\r': out += "\\r";  break;
                    case '\t': out += "\\t";  break;
                    default:
                        if (static_cast<unsigned char>(c) < 0x20) {
                            char buf[8];
                            std::snprintf(buf, sizeof(buf), "\\u%04X",
                                          static_cast<unsigned>(c));
                            out += buf;
                        } else {
                            out += c;
                        }
                }
            }
            return out;
        }

        std::string BuildJson() {
            std::string out = "{\n";
            out += "  \"build_id\": \"" + EscapeJson(g_state.buildId) + "\",\n";

            for (const auto& [subdir, sha] : g_state.shas) {
                out += "  \"" + EscapeJson(subdir) + "_sha\": \""
                     + EscapeJson(sha) + "\",\n";
            }
            for (const auto& [subdir, found] : g_state.tomlFound) {
                out += "  \"" + EscapeJson(subdir) + "_toml_found\": "
                     + (found ? "true" : "false") + ",\n";
            }

            auto emitArray = [&](const char* key, const std::set<std::string>& s) {
                out += "  \"";
                out += key;
                out += "\": [";
                bool first = true;
                for (const auto& name : s) {
                    if (!first) out += ", ";
                    out += "\"" + EscapeJson(name) + "\"";
                    first = false;
                }
                out += "],\n";
            };
            emitArray("hooks_installed", g_state.hits);
            emitArray("hooks_missed",    g_state.misses);

            const auto now = std::chrono::system_clock::now().time_since_epoch();
            const auto secs = std::chrono::duration_cast<std::chrono::seconds>(now).count();
            out += "  \"ts\": " + std::to_string(secs) + "\n";
            out += "}\n";
            return out;
        }
    } // anonymous namespace

    void Init(const std::string& buildId) {
        std::lock_guard<std::mutex> lk(g_mu);
        g_state.buildId = buildId;
    }

    void RecordTomlState(const char* subdir, const std::string& sha, bool tomlFound) {
        if (!subdir) return;
        std::lock_guard<std::mutex> lk(g_mu);
        g_state.shas[subdir]      = sha;
        g_state.tomlFound[subdir] = tomlFound;
    }

    void RecordHit(const char* funcName) {
        if (!funcName) return;
        std::lock_guard<std::mutex> lk(g_mu);
        g_state.hits.insert(funcName);
        g_state.misses.erase(funcName);
    }

    void RecordMiss(const char* funcName) {
        if (!funcName) return;
        std::lock_guard<std::mutex> lk(g_mu);
        g_state.misses.insert(funcName);
        // Don't erase from hits — a later successful retry takes precedence.
    }

    void Flush() {
        std::string body;
        {
            std::lock_guard<std::mutex> lk(g_mu);
            body = BuildJson();
        }

        if (!SteamInstallPath[0]) {
            LOG_MISC_DEBUG("StatusWriter::Flush: SteamInstallPath unset, skipping");
            return;
        }

        std::filesystem::path dir = std::filesystem::path(SteamInstallPath) / "lumacore";
        std::error_code ec;
        std::filesystem::create_directories(dir, ec);
        if (ec) {
            LOG_MISC_DEBUG("StatusWriter::Flush: create_directories failed: {}",
                           ec.message());
            return;
        }

        std::filesystem::path path    = dir / "status.json";
        std::filesystem::path tmpPath = path;
        tmpPath += ".tmp";

        {
            std::ofstream f(tmpPath, std::ios::binary | std::ios::trunc);
            if (!f) {
                LOG_MISC_DEBUG("StatusWriter::Flush: open tmp failed: {}",
                               tmpPath.string());
                return;
            }
            f.write(body.data(), static_cast<std::streamsize>(body.size()));
            if (!f) {
                LOG_MISC_DEBUG("StatusWriter::Flush: write tmp failed");
                return;
            }
        }

        std::string narrowTmp  = tmpPath.string();
        std::string narrowPath = path.string();
        if (!MoveFileExA(narrowTmp.c_str(), narrowPath.c_str(),
                         MOVEFILE_REPLACE_EXISTING)) {
            DWORD err = GetLastError();
            LOG_MISC_DEBUG("StatusWriter::Flush: MoveFileExA failed err={}", err);
            std::error_code rmEc;
            std::filesystem::remove(tmpPath, rmEc);
        }
    }
}
