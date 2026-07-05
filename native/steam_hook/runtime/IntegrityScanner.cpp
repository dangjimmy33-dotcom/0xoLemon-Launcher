// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "IntegrityScanner.h"
#include "Logger.h"

#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <psapi.h>
#include <TlHelp32.h>
#include <algorithm>
#include <array>
#include <cctype>
#include <chrono>
#include <cmath>
#include <filesystem>
#include <fstream>
#include <span>
#include <vector>

namespace ProtectionScan {

    namespace {

        constexpr std::array<std::string_view, 5> kLegacyDenuvoSections = {
            ".arch", ".srdata", ".xpdata", ".xdata", ".xtls",
        };

        // "DODENUNOVO" pattern at OEP area
        constexpr std::array<uint8_t, 10> kDenuvoOepPattern = {
            0x48, 0xB9, 0x44, 0x4F, 0x44, 0x45, 0x4E, 0x55, 0x56, 0x4F,
        };

        constexpr uint32_t kMinPackedModuleBytes = 80u * 1024u * 1024u;
        constexpr std::array<std::string_view, 10> kSteamRuntimeModules = {
            "steamclient.dll", "steamclient64.dll",
            "steam_api.dll", "steam_api64.dll",
            "tier0_s.dll", "tier0_s64.dll",
            "vstdlib_s.dll", "vstdlib_s64.dll",
            "gameoverlayrenderer.dll", "gameoverlayrenderer64.dll",
        };

        std::string Lower(std::string s) {
            for (char& c : s) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
            return s;
        }

        std::string BaseName(std::string_view path) {
            auto slash = path.find_last_of("\\/");
            return slash == std::string_view::npos ? std::string(path) : std::string(path.substr(slash + 1));
        }

        std::string DirectoryName(std::string_view path) {
            auto slash = path.find_last_of("\\/");
            return slash == std::string_view::npos ? std::string() : std::string(path.substr(0, slash + 1));
        }

        bool EndsWithInsensitive(std::string_view value, std::string_view suffix) {
            if (value.size() < suffix.size()) return false;
            size_t offset = value.size() - suffix.size();
            for (size_t i = 0; i < suffix.size(); ++i) {
                if (std::tolower(static_cast<unsigned char>(value[offset + i])) !=
                    std::tolower(static_cast<unsigned char>(suffix[i])))
                    return false;
            }
            return true;
        }

        bool IsSteamRuntimeModule(std::string_view name) {
            std::string n = Lower(std::string(name));
            for (auto rn : kSteamRuntimeModules)
                if (n == rn) return true;
            return false;
        }

        struct ModuleInfo {
            std::string path;
            uint32_t size = 0;
            bool executable = false;
        };

        std::vector<ModuleInfo> EnumerateModules(uint32_t pid) {
            std::vector<ModuleInfo> result;

            HANDLE hSnap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);
            if (hSnap == INVALID_HANDLE_VALUE) return result;

            MODULEENTRY32W me{};
            me.dwSize = sizeof(me);
            if (Module32FirstW(hSnap, &me)) {
                do {
                    char path[MAX_PATH];
                    WideCharToMultiByte(CP_UTF8, 0, me.szExePath, -1, path, MAX_PATH, nullptr, nullptr);

                    bool exe = EndsWithInsensitive(path, ".exe");
                    bool dll = EndsWithInsensitive(path, ".dll");
                    if (!exe && !dll) continue;
                    if (!exe && me.modBaseSize < kMinPackedModuleBytes) continue;

                    if (!exe && IsSteamRuntimeModule(BaseName(path))) continue;

                    result.push_back({path, me.modBaseSize, exe});
                } while (Module32NextW(hSnap, &me));
            }
            CloseHandle(hSnap);

            // Sort: exe first, then by size descending
            std::stable_sort(result.begin(), result.end(),
                [](const ModuleInfo& a, const ModuleInfo& b) {
                    if (a.executable != b.executable) return a.executable;
                    return a.size > b.size;
                });

            return result;
        }
    }

    const char* ToString(Method m) {
        switch (m) {
        case Method::None: return "None";
        case Method::LegacySectionString: return "LegacySectionString";
        case Method::OepPattern: return "OepPattern";
        case Method::ProtectedBlobSection: return "ProtectedBlobSection";
        }
        return "?";
    }

    double SectionEntropy(std::span<const uint8_t> bytes) {
        if (bytes.empty()) return 0.0;
        int counts[256] = {};
        for (uint8_t b : bytes) ++counts[b];
        double n = static_cast<double>(bytes.size());
        double ent = 0.0;
        for (int c : counts)
            if (c > 0) ent -= (c / n) * std::log2(c / n);
        return ent;
    }

    struct BlobMatch { std::string sectionName; size_t rawOff = 0; };

    std::optional<BlobMatch> TryProtectedBlobSection(const ModuleInfo& mod,
                                                     const std::vector<uint8_t>& peData) {
        constexpr size_t kBlobMin = 4 * 1024 * 1024;
        constexpr size_t kSampleMax = 8 * 1024 * 1024;
        constexpr double kMinEntropy = 6.0;

        if (peData.size() < 0x200) return std::nullopt;
        const uint8_t* base = peData.data();
        uint32_t e_lfanew = *reinterpret_cast<const uint32_t*>(base + 0x3C);
        if (e_lfanew + 4 > peData.size()) return std::nullopt;
        if (memcmp(base + e_lfanew, "PE\0\0", 4) != 0) return std::nullopt;
        uint32_t peOff = e_lfanew + 4;
        uint16_t numSections = *reinterpret_cast<const uint16_t*>(base + peOff + 2);
        uint16_t optHdrSize = *reinterpret_cast<const uint16_t*>(base + peOff + 16);
        uint32_t secOff = peOff + 20 + optHdrSize;

        for (uint16_t i = 0; i < numSections; ++i) {
            const uint8_t* sec = base + secOff + i * 40;
            uint32_t flags = *reinterpret_cast<const uint32_t*>(sec + 36);
            if ((flags & 0xA0000000) != 0xA0000000) continue;
            uint32_t rawSize = *reinterpret_cast<const uint32_t*>(sec + 16);
            if (rawSize < kBlobMin) continue;

            uint32_t rawOff = *reinterpret_cast<const uint32_t*>(sec + 20);
            size_t sampleSz = (std::min)(static_cast<size_t>(rawSize), kSampleMax);
            if (rawOff + sampleSz > peData.size()) continue;
            std::span<const uint8_t> sample(base + rawOff, sampleSz);
            double ent = SectionEntropy(sample);
            if (ent < kMinEntropy) continue;

            char secName[9] = {};
            memcpy(secName, sec, 8);
            LOG_MISC_INFO("ProtectionScan: RWX blob module={} section={} size={} ({:.1f} MB) entropy={:.3f}",
                          mod.path, secName, rawSize, rawSize / (1024.0 * 1024.0), ent);
            return BlobMatch{secName, rawOff};
        }
        return std::nullopt;
    }

    Report Scan(uint32_t pid) {
        Report report{};
        if (pid == 0) return report;

        auto start = std::chrono::steady_clock::now();
        auto modules = EnumerateModules(pid);
        report.pid = pid;

        for (const auto& mod : modules) {
            ++report.scannedModules;

            // Search for the OEP pattern directly in the file
            std::filesystem::path nativePath(mod.path);
            if (!std::filesystem::exists(nativePath)) continue;

            std::ifstream file(nativePath, std::ios::binary);
            if (!file) continue;
            file.seekg(0, std::ios::end);
            std::streamsize fsize = file.tellg();
            file.seekg(0, std::ios::beg);
            if (fsize <= 0 || static_cast<size_t>(fsize) > 1024u * 1024u * 256u) continue;

            std::vector<uint8_t> buf(static_cast<size_t>(fsize));
            if (!file.read(reinterpret_cast<char*>(buf.data()), fsize)) continue;

            // Scan for the OEP byte sequence
            auto it = std::search(buf.begin(), buf.end(),
                                  kDenuvoOepPattern.begin(), kDenuvoOepPattern.end());
            if (it != buf.end()) {
                size_t offset = static_cast<size_t>(std::distance(buf.begin(), it));
                report.denuvoDetected = true;
                report.method = Method::OepPattern;
                report.modulePath = mod.path;
                report.moduleSize = mod.size;
                report.matchRawOffset = offset;
                report.elapsedMs = std::chrono::duration<double, std::milli>(
                    std::chrono::steady_clock::now() - start).count();

                LOG_MISC_INFO("ProtectionScan: Denuvo detected via OEP pattern pid={} module={} offset=0x{:X}",
                               pid, mod.path, offset);
                return report;
            }

            // Protected blob: W+X section with high entropy (catches Denuvo builds
            // that have no OEP pattern and no legacy section string)
            if (auto blob = TryProtectedBlobSection(mod, buf)) {
                report.denuvoDetected = true;
                report.method = Method::ProtectedBlobSection;
                report.modulePath = mod.path;
                report.moduleSize = mod.size;
                report.sectionName = blob->sectionName;
                report.matchRawOffset = blob->rawOff;
                report.elapsedMs = std::chrono::duration<double, std::milli>(
                    std::chrono::steady_clock::now() - start).count();
                return report;
            }

            // Fallback: check for legacy Denuvo sections + scan for DENUVO string
            // Try reading the PE header to find legacy sections
            std::vector<uint8_t> peData;
            {
                FILE* f = nullptr;
                if (fopen_s(&f, mod.path.c_str(), "rb") == 0 && f) {
                    fseek(f, 0, SEEK_END);
                    long fsize = ftell(f);
                    if (fsize > 0 && static_cast<size_t>(fsize) <= 1024u * 1024u * 64u) {
                        peData.resize(static_cast<size_t>(fsize));
                        fseek(f, 0, SEEK_SET);
                        fread(peData.data(), 1, peData.size(), f);
                    }
                    fclose(f);
                }
            }

            if (peData.empty()) continue;

            // Crude section name check for legacy Denuvo
            if (peData.size() < 0x200) continue;
            uint16_t numSections = 0;
            uint16_t optHdrSize = 0;
            size_t sectionOffset = 0;

            // Parse PE header
            uint16_t magic = *reinterpret_cast<const uint16_t*>(peData.data());
            if (magic != 0x5A4D) continue; // MZ

            uint32_t peOffset = *reinterpret_cast<const uint32_t*>(peData.data() + 0x3C);
            if (peOffset + 4 >= peData.size()) continue;

            uint32_t peSig = *reinterpret_cast<const uint32_t*>(peData.data() + peOffset);
            if (peSig != 0x00004550) continue; // PE\0\0

            uint16_t machine = *reinterpret_cast<const uint16_t*>(peData.data() + peOffset + 4);
            (void)machine;
            numSections = *reinterpret_cast<const uint16_t*>(peData.data() + peOffset + 6);
            optHdrSize = *reinterpret_cast<const uint16_t*>(peData.data() + peOffset + 0x14);
            sectionOffset = peOffset + 0x18 + optHdrSize;

            for (uint16_t i = 0; i < numSections; ++i) {
                size_t secAddr = sectionOffset + i * 40;
                if (secAddr + 40 > peData.size()) break;

                char secName[9] = {};
                memcpy(secName, peData.data() + secAddr, 8);

                // Check if this is a known Denuvo section
                bool isDenuvoSection = false;
                for (auto dn : kLegacyDenuvoSections) {
                    if (secName == dn) { isDenuvoSection = true; break; }
                }
                if (!isDenuvoSection) continue;

                // Found a Denuvo section, now scan for the DENUVO string within the module's mapped range
                uint32_t secRawSize = *reinterpret_cast<const uint32_t*>(peData.data() + secAddr + 16);
                uint32_t secRawOffset = *reinterpret_cast<const uint32_t*>(peData.data() + secAddr + 20);

                if (secRawSize == 0) continue;

                // Scan for "DENUVO" string in this section
                size_t end = (std::min)(static_cast<size_t>(secRawOffset) + secRawSize, peData.size());
                size_t pos = secRawOffset;
                const std::string_view denuvoStr = "DENUVO";
                for (; pos + denuvoStr.size() <= end; ++pos) {
                    if (memcmp(peData.data() + pos, denuvoStr.data(), denuvoStr.size()) == 0) {
                        report.denuvoDetected = true;
                        report.method = Method::LegacySectionString;
                        report.modulePath = mod.path;
                        report.sectionName = secName;
                        report.moduleSize = mod.size;
                        report.matchRawOffset = pos;
                        report.elapsedMs = std::chrono::duration<double, std::milli>(
                            std::chrono::steady_clock::now() - start).count();

                        LOG_MISC_INFO("ProtectionScan: Denuvo detected via legacy section pid={} module={} section={}",
                                       pid, mod.path, secName);
                        return report;
                    }
                }
            }
        }

        report.elapsedMs = std::chrono::duration<double, std::milli>(
            std::chrono::steady_clock::now() - start).count();
        LOG_MISC_DEBUG("ProtectionScan: pid={} no Denuvo found scanned={} elased={:.3f}ms",
                        pid, report.scannedModules, report.elapsedMs);
        return report;
    }

}
