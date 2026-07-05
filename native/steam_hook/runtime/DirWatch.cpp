// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "core/entry.h"
#include "DirWatch.h"
#include "config/LuaLoader.h"
#include "hooks/capture/SteamCapture.h"
#include "Logger.h"
#include <atomic>
#include <filesystem>
#include <string>
#include <thread>
#include <unordered_map>
#include <vector>

namespace DirWatch {

    static constexpr DWORD kBufBytes      = 65536;
    static constexpr DWORD kDebounceMs    = 500;
    // ENUM_DIR fires when the kernel buffer overflows on a hot burst (vin
    // hit this dropping 160 .luas at once). Slot has to be torn down and
    // re-opened, ReadDirectoryChangesW won't recover on its own.
    static constexpr int kSlotRecoveryBudget = 3;

    // Build the same string ParseDirectory hands LuaLoader::ParseFile, so
    // the g_fileDepots key from the boot scan matches the one Harvest
    // produces at runtime. Without this UnloadFile silently no-ops on
    // removes whenever the dir came in with mixed slashes or a trailing
    // backslash. Keep unicode segments untouched, just normalize the
    // separators and collapse any "." / ".." segments.
    static std::string NormalizeFullPath(const std::string& dir, const std::string& name) {
        std::filesystem::path p = std::filesystem::path(dir) / name;
        return p.lexically_normal().make_preferred().string();
    }

    // ── WatchSlot: encapsulates all per-directory watch state ──────────────────
    // Each monitored directory gets one slot. Open() acquires the directory handle
    // and arms the first overlapped read. Harvest() drains one completed read into
    // the caller-supplied accumulator map (full path -> action) and immediately
    // re-arms. Close() tears down the slot cleanly.
    struct WatchSlot {
        std::string path;
        HANDLE      hDir   = nullptr;
        HANDLE      hEvent = nullptr;
        OVERLAPPED  ov     = {};
        char        buf[kBufBytes]{};
        // bumped every time Reopen() rebuilds the slot. When this hits
        // kSlotRecoveryBudget the slot is dead and we drop it.
        int         recoveryAttempts = 0;
        bool        dead             = false;

        bool Open() {
            hEvent = CreateEventA(nullptr, FALSE, FALSE, nullptr);
            if (!hEvent) return false;
            ov.hEvent = hEvent;

            hDir = CreateFileA(path.c_str(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                nullptr, OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                nullptr);
            if (hDir == INVALID_HANDLE_VALUE) {
                LOG_PKGCH_WARN("DirWatch: failed to open '{}' (err={})", path, GetLastError());
                CloseHandle(hEvent);
                hDir = hEvent = nullptr;
                return false;
            }
            return Arm();
        }

        bool Arm() {
            DWORD nb = 0;
            if (!ReadDirectoryChangesW(hDir, buf, kBufBytes, FALSE,
                                       FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE,
                                       &nb, &ov, nullptr)) {
                DWORD err = GetLastError();
                if (err == ERROR_IO_PENDING) return true;
                LOG_PKGCH_WARN("DirWatch: ReadDirectoryChangesW failed (err={})", err);
                return false;
            }
            return true;
        }

        // Recover from a buffer-overflow style failure (ENUM_DIR). Close
        // everything down and re-open the same path. Returns true if the
        // slot is alive again, false if we burned through the retry budget.
        bool Reopen() {
            if (recoveryAttempts >= kSlotRecoveryBudget) {
                LOG_PKGCH_WARN(
                    "DirWatch: slot for '{}' burned through {} recovery attempt(s), dropping",
                    path, kSlotRecoveryBudget);
                dead = true;
                return false;
            }
            ++recoveryAttempts;
            LOG_PKGCH_INFO(
                "DirWatch: re-opening slot for '{}' (attempt {}/{})",
                path, recoveryAttempts, kSlotRecoveryBudget);

            if (hDir && hDir != INVALID_HANDLE_VALUE) { CloseHandle(hDir); hDir = nullptr; }
            if (hEvent) { CloseHandle(hEvent); hEvent = nullptr; }
            ov = {};
            std::fill(std::begin(buf), std::end(buf), '\0');
            return Open();
        }

        // Drain one completed overlapped result into acc (full-path -> last-action map).
        // Re-arms immediately so events that arrive during the debounce window aren't lost.
        // Returns true on a clean drain, false when the slot needs recovery.
        bool Harvest(std::unordered_map<std::string, DWORD>& acc,
                     std::vector<std::string>& ordering)
        {
            DWORD nb = 0;
            if (!GetOverlappedResult(hDir, &ov, &nb, FALSE)) {
                DWORD err = GetLastError();
                // ENUM_DIR / INVALID_USER_BUFFER mean the kernel-side change
                // buffer overflowed under burst load. Re-arming the same
                // handle is useless; the slot needs a clean reopen.
                if (err == ERROR_NOTIFY_ENUM_DIR || err == ERROR_INVALID_USER_BUFFER) {
                    LOG_PKGCH_WARN(
                        "DirWatch: slot for '{}' overflowed (err={}), forcing re-open",
                        path, err);
                    return false;
                }
                Arm();
                return true;
            }
            if (!nb) {
                // Zero-byte completion is the kernel's other "you missed
                // events, here's nothing" signal under load. Treat the
                // same as overflow.
                LOG_PKGCH_WARN("DirWatch: slot for '{}' returned 0 bytes, forcing re-open", path);
                return false;
            }

            const FILE_NOTIFY_INFORMATION* rec =
                reinterpret_cast<const FILE_NOTIFY_INFORMATION*>(buf);
            while (rec) {
                DWORD act = rec->Action;
                if (act == FILE_ACTION_ADDED || act == FILE_ACTION_MODIFIED
                        || act == FILE_ACTION_REMOVED) {
                    std::wstring_view fn(rec->FileName, rec->FileNameLength / sizeof(wchar_t));
                    if (fn.size() >= 4 && fn.substr(fn.size() - 4) == L".lua") {
                        std::string name(fn.size(), '\0');
                        for (size_t i = 0; i < fn.size(); ++i)
                            name[i] = static_cast<char>(fn[i]);
                        // Canonicalize before pushing so the key matches
                        // whatever ParseDirectory wrote on boot. Without
                        // this UnloadFile silently misses on a slash flip.
                        std::string full = NormalizeFullPath(path, name);
                        LOG_PKGCH_INFO("Lua file {}: {}",
                            act == FILE_ACTION_ADDED    ? "added"    :
                            act == FILE_ACTION_MODIFIED ? "modified" : "removed", name);
                        if (!acc.count(full)) ordering.push_back(full);
                        acc[full] = act;
                    }
                }
                if (!rec->NextEntryOffset) break;
                rec = reinterpret_cast<const FILE_NOTIFY_INFORMATION*>(
                    reinterpret_cast<const char*>(rec) + rec->NextEntryOffset);
            }
            // Healthy slot, re-arm for the next event burst. Reset the
            // recovery counter on a successful drain so a slot that
            // recovered once and then ran clean for a while gets its
            // full budget back if a new burst hits later.
            recoveryAttempts = 0;
            if (!Arm()) {
                // Arm itself failed cleanly (queued IO_PENDING is the
                // success path). Anything else means the handle is sick.
                return false;
            }
            return true;
        }

        void Close() {
            if (hDir && hDir != INVALID_HANDLE_VALUE) { CloseHandle(hDir); hDir = nullptr; }
            if (hEvent) { CloseHandle(hEvent); hEvent = nullptr; }
        }

        bool Valid() const { return !dead && hDir && hDir != INVALID_HANDLE_VALUE; }
    };

    static std::atomic<bool> g_alive{false};
    static std::thread        g_MonitorThread;
    static std::vector<std::string> g_dirs;

    // Rebuild the evts/idxMap pair after a slot reopens (or dies). Cheap,
    // only runs after a recovery, never on the hot path.
    static void RebuildWaitTables(std::vector<WatchSlot>& slots,
                                  std::vector<HANDLE>& evts,
                                  std::vector<size_t>& idxMap)
    {
        evts.clear();
        idxMap.clear();
        for (size_t i = 0; i < slots.size(); ++i) {
            if (slots[i].Valid()) {
                evts.push_back(slots[i].hEvent);
                idxMap.push_back(i);
            }
        }
        if (evts.size() > MAXIMUM_WAIT_OBJECTS) {
            evts.resize(MAXIMUM_WAIT_OBJECTS);
            idxMap.resize(MAXIMUM_WAIT_OBJECTS);
        }
    }

    // ── MonitorThread ──────────────────────────────────────────────────────────
    static void MonitorThread()
    {
        // Build one WatchSlot per directory.
        std::vector<WatchSlot> slots(g_dirs.size());
        for (size_t i = 0; i < slots.size(); ++i) {
            slots[i].path = g_dirs[i];
            if (slots[i].Open())
                LOG_PKGCH_INFO("DirWatch: watching '{}'", g_dirs[i]);
        }

        std::vector<HANDLE> evts;
        std::vector<size_t> idxMap;
        evts.reserve(slots.size());
        idxMap.reserve(slots.size());
        RebuildWaitTables(slots, evts, idxMap);

        // Win32 caps WaitForMultipleObjects at MAXIMUM_WAIT_OBJECTS handles per call.
        if (slots.size() > MAXIMUM_WAIT_OBJECTS) {
            LOG_PKGCH_WARN("DirWatch: directory count {} exceeds Win32 wait limit {}, truncating",
                           slots.size(), static_cast<size_t>(MAXIMUM_WAIT_OBJECTS));
        }

        if (evts.empty()) {
            LOG_PKGCH_WARN("DirWatch: no directories could be opened, watcher exiting");
            for (auto& s : slots) s.Close();
            return;
        }

        while (g_alive) {
            DWORD nEvts = static_cast<DWORD>(evts.size());
            DWORD wr = WaitForMultipleObjects(nEvts, evts.data(), FALSE, 1000);
            if (!g_alive) break;
            if (wr == WAIT_TIMEOUT) continue;
            if (wr < WAIT_OBJECT_0 || wr >= WAIT_OBJECT_0 + nEvts) continue;

            std::unordered_map<std::string, DWORD> acc;
            std::vector<std::string> ordering;

            size_t firstSlot = idxMap[wr - WAIT_OBJECT_0];
            bool needRebuild = false;
            if (!slots[firstSlot].Harvest(acc, ordering)) {
                if (!slots[firstSlot].Reopen()) {
                    // Slot is dead. Mark it, rebuild wait tables, and
                    // skip ahead to the next event source.
                }
                needRebuild = true;
            }

            // Debounce: keep draining until a quiet period of kDebounceMs.
            while (g_alive) {
                if (needRebuild) {
                    RebuildWaitTables(slots, evts, idxMap);
                    nEvts = static_cast<DWORD>(evts.size());
                    needRebuild = false;
                    if (evts.empty()) break;
                }
                DWORD dr = WaitForMultipleObjects(nEvts, evts.data(), FALSE, kDebounceMs);
                if (!g_alive || dr == WAIT_TIMEOUT) break;
                if (dr < WAIT_OBJECT_0 || dr >= WAIT_OBJECT_0 + nEvts) break;
                size_t slotIx = idxMap[dr - WAIT_OBJECT_0];
                if (!slots[slotIx].Harvest(acc, ordering)) {
                    if (!slots[slotIx].Reopen()) {
                        // Burned out, falls through to RebuildWaitTables
                        // on the next loop iteration which will drop it
                        // out of evts/idxMap.
                    }
                    needRebuild = true;
                }
            }

            if (needRebuild) {
                RebuildWaitTables(slots, evts, idxMap);
                if (evts.empty()) {
                    LOG_PKGCH_WARN("DirWatch: every slot died, watcher exiting");
                    break;
                }
            }

            if (!ordering.empty()) {
                LOG_PKGCH_INFO("DirWatch: processing {} Lua file change(s)", ordering.size());
                for (const auto& fullPath : ordering) {
                    if (acc[fullPath] == FILE_ACTION_REMOVED)
                        LuaLoader::UnloadFile(fullPath);
                    else
                        LuaLoader::ParseFile(fullPath);
                }
                SteamCapture::NotifyLicenseChanged();
                LOG_PKGCH_INFO("DirWatch: refresh completed");
            }
        }

        for (auto& s : slots) s.Close();
        LOG_PKGCH_INFO("DirWatch: stopped");
    }

    void Start(const std::vector<std::string>& directories) {
        if (directories.empty()) {
            LOG_PKGCH_WARN("DirWatch::Start: no directories configured, watcher not dispatched");
            return;
        }
        if (g_alive.exchange(true)) {
            LOG_PKGCH_WARN("DirWatch: already running");
            return;
        }
        // Normalize the watched dirs the same way Harvest normalizes the
        // per-event paths. Boot scan in entry.cpp already calls
        // ParseDirectory(dir) with the raw setting value, but the
        // directory_iterator inside ParseDirectory yields native paths,
        // so normalizing here keeps the slot path stable for log output
        // without changing what ParseDirectory walks.
        g_dirs.clear();
        g_dirs.reserve(directories.size());
        for (const auto& d : directories) {
            try {
                g_dirs.push_back(std::filesystem::path(d).lexically_normal().make_preferred().string());
            } catch (...) {
                g_dirs.push_back(d);
            }
        }
        g_MonitorThread = std::thread(MonitorThread);
    }

    void Stop() {
        if (!g_alive) return;
        g_alive = false;
        if (g_MonitorThread.joinable())
            g_MonitorThread.join();
    }
}
