// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "Diagnostics.h"

#ifdef LUMACORE_DIAGNOSTICS_ENABLED

#include <atomic>
#include <chrono>
#include <cstdio>
#include <cstring>

#include <windows.h>
#include <shlobj.h>
#include <knownfolders.h>

#include "core/entry.h"

namespace Diagnostics {

    namespace {

        constexpr uint32_t kRingSize = 64;

        // Single allocation, fixed at TU scope. No heap, no pools.
        DiagEntry              g_ring[kRingSize] = {};
        std::atomic<uint32_t>  g_writeIndex{0};

        // Cached SteamID64 captured at first Record/Dump so the dump header
        // shows the active account even when called from Detach paths where
        // Ticket helpers may already be torn down. 0 means "not captured".
        std::atomic<uint64_t>  g_cachedSteamId{0};

        uint64_t NowMs() noexcept {
            return static_cast<uint64_t>(
                std::chrono::duration_cast<std::chrono::milliseconds>(
                    std::chrono::system_clock::now().time_since_epoch()
                ).count()
            );
        }

        // Resolve <AppData Roaming>\\SteaMidra\\lumacore_diag.txt.
        // The buffer is filled even when AppData lookup fails so callers can
        // still log the path they tried; on failure the function returns
        // false and the caller skips the actual write.
        bool ResolveDumpPath(char* out, size_t cap) noexcept {
            if (!out || cap < 4) return false;
            out[0] = '\0';

            PWSTR pszPath = nullptr;
            HRESULT hr = SHGetKnownFolderPath(
                FOLDERID_RoamingAppData, 0, nullptr, &pszPath);
            if (FAILED(hr) || !pszPath) {
                if (pszPath) CoTaskMemFree(pszPath);
                return false;
            }

            // Convert the wide AppData path to ANSI for the rest of the
            // function. Best-effort: characters outside the active code
            // page are dropped. Diagnostic dumps are recovery aids, not
            // primary I/O — silent failure is the right behaviour here.
            char appdataA[MAX_PATH] = {};
            int rc = WideCharToMultiByte(
                CP_ACP, 0, pszPath, -1, appdataA, MAX_PATH, nullptr, nullptr);
            CoTaskMemFree(pszPath);
            if (rc <= 0) return false;

            int written = std::snprintf(
                out, cap, "%s\\SteaMidra\\lumacore_diag.txt", appdataA);
            if (written <= 0 || static_cast<size_t>(written) >= cap)
                return false;

            // Make sure the parent directory exists. Best-effort, ignores
            // errors (CreateDirectoryA returns 0 if the dir already exists,
            // which is fine here — actual write failure surfaces below).
            char parent[MAX_PATH] = {};
            std::snprintf(parent, MAX_PATH, "%s\\SteaMidra", appdataA);
            CreateDirectoryA(parent, nullptr);
            return true;
        }

        const char* SurfaceName(uint8_t s) noexcept {
            switch (static_cast<Surface>(s)) {
                case Surface::Callback: return "callback";
                case Surface::EMsgRecv: return "emsg_recv";
                case Surface::EMsgSend: return "emsg_send";
            }
            return "unknown";
        }

        const char* ActionName(uint8_t a) noexcept {
            switch (static_cast<Action>(a)) {
                case Action::Drop:        return "drop";
                case Action::Forward:     return "forward";
                case Action::Strip:       return "strip";
                case Action::PassThrough: return "pass";
            }
            return "unknown";
        }

    } // namespace

    void Record(Surface s, uint32_t code, uint32_t appid, Action a) noexcept {
        // fetch_add returns the OLD value, so we slot the entry there and
        // let other callers race ahead. Wraparound modulo 64 keeps the
        // window bounded. Memory order is relaxed because the ring is a
        // best-effort capture: a torn read in Dump is acceptable and the
        // dump runs on a quiescent path (detach / menu action).
        uint32_t idx = g_writeIndex.fetch_add(1, std::memory_order_relaxed)
                     % kRingSize;

        DiagEntry e = {};
        e.timestamp_ms = NowMs();
        e.surface      = static_cast<uint8_t>(s);
        e.code         = code;
        e.appid        = appid;
        e.action       = static_cast<uint8_t>(a);
        g_ring[idx]    = e;
    }

    bool Dump(const char* reason) noexcept {
        char path[MAX_PATH] = {};
        if (!ResolveDumpPath(path, MAX_PATH)) return false;

        FILE* fp = nullptr;
        if (fopen_s(&fp, path, "ab") != 0 || !fp) return false;

        // Header: LumaCore version + active SteamID64 + reason + UTC ts.
        // Version comes from the project header (kept hard-coded here so
        // the diagnostic does not pull in the heavier Settings module).
        const uint64_t steamId = g_cachedSteamId.load(std::memory_order_relaxed);
        const uint64_t now     = NowMs();
        std::fprintf(
            fp,
            "[lumacore-diag] reason=%s steamid64=%llu ts_ms=%llu entries=%u\n",
            reason ? reason : "?",
            static_cast<unsigned long long>(steamId),
            static_cast<unsigned long long>(now),
            kRingSize
        );

        // Walk the ring oldest-first. Current write index points at the
        // next slot; the slot before it is the most-recent entry.
        uint32_t writeIdx = g_writeIndex.load(std::memory_order_relaxed);
        for (uint32_t i = 0; i < kRingSize; ++i) {
            uint32_t idx = (writeIdx + i) % kRingSize;
            const DiagEntry& e = g_ring[idx];
            if (e.timestamp_ms == 0) continue;  // never written
            std::fprintf(
                fp,
                "  ts=%llu surface=%s code=%u appid=%u action=%s\n",
                static_cast<unsigned long long>(e.timestamp_ms),
                SurfaceName(e.surface),
                e.code,
                e.appid,
                ActionName(e.action)
            );
        }
        std::fputc('\n', fp);
        std::fclose(fp);
        return true;
    }

    void DumpForDetach() noexcept {
        Dump("detach");
    }

} // namespace Diagnostics

#endif  // LUMACORE_DIAGNOSTICS_ENABLED
