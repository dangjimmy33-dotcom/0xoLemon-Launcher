// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "runtime/BootDiag.h"

#include "core/entry.h"
#include "runtime/Logger.h"
#include "config/Settings.h"

#include <windows.h>
#include <bcrypt.h>

#include <array>
#include <cstdint>
#include <cstdio>
#include <string>
#include <thread>
#include <vector>

#pragma comment(lib, "bcrypt.lib")

namespace BootDiag {

    namespace {

        std::string g_capturedBuildId;
        std::string g_capturedSha;

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

        std::string Sha256OfFile(const char* path) {
            HANDLE hFile = CreateFileA(path, GENERIC_READ,
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

        void PopupThread() {
            char msg[4096];
            std::snprintf(msg, sizeof(msg),
                "LumaCore: IPC specs unavailable\n\n"
                "Steam build ID: %s\n"
                "Steamclient:    %s\n\n"
                "This Steam version may not be supported yet. "
                "Some game features may not work correctly.\n\n"
                "This is a read-only diagnostic -- your files are not affected.",
                g_capturedBuildId.empty() ? "unknown" : g_capturedBuildId.c_str(),
                g_capturedSha.empty()     ? "unknown" : g_capturedSha.c_str());

            MessageBoxA(nullptr, msg, "LumaCore -- Steam Diagnostics",
                        MB_OK | MB_ICONWARNING | MB_SETFOREGROUND);
        }

    } // anonymous namespace

    void Capture() {
        g_capturedBuildId = g_steamBuildId;
        g_capturedSha     = Sha256OfFile(SteamclientPath);

        LOG_MISC_DEBUG("BootDiag: captured build={} sha={}",
                       g_capturedBuildId.empty() ? "unknown" : g_capturedBuildId,
                       g_capturedSha.empty()     ? "unknown" : g_capturedSha);
    }

    void ReportMissing() {
        if (!Settings::diagnosticPopupEnabled) {
            LOG_MISC_DEBUG("BootDiag: popup disabled, skipping");
            return;
        }
        std::thread(PopupThread).detach();
        LOG_MISC_DEBUG("BootDiag: popup dispatched");
    }

} // namespace BootDiag
