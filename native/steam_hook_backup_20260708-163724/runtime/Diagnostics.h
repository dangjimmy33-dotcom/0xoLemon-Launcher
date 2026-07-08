// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

// A16 achievement diagnostic ring buffer.
//
// Captures the last 64 achievement-adjacent events (callbacks 1102/1103,
// EMsg 147 strip / pass-through, EMsgs 817-819 record-only) for triage on
// shipped Release builds. The ring lives behind LUMACORE_DIAGNOSTICS_ENABLED
// and is on for both Debug and Release in the standard build, so users who
// hit the Wukong reproducer can dump it from the SteaMidra Help menu and
// share the file.
//
// Hot path: Record() is lock-free, allocation-free, and never performs I/O.
// One std::atomic<uint32_t> write index with wraparound modulo 64. The dump
// path opens the file lazily, writes a header line plus the ring contents,
// and fails silently when AppData is missing or the file is not writable.

#include <cstdint>

#ifdef LUMACORE_DIAGNOSTICS_ENABLED

namespace Diagnostics {

    enum class Surface : uint8_t {
        Callback = 1,
        EMsgRecv = 2,
        EMsgSend = 3,
    };

    enum class Action : uint8_t {
        Drop        = 1,
        Forward     = 2,
        Strip       = 3,
        PassThrough = 4,
    };

    // 40-byte POD entry. No STL containers, no destructors, no virtuals.
    struct DiagEntry {
        uint64_t timestamp_ms;
        uint8_t  surface;       // Surface enum value
        uint32_t code;          // callback id or EMsg
        uint32_t appid;
        uint8_t  action;        // Action enum value
        uint8_t  pad[2];        // explicit padding so layout is well-defined
    };

    // Push one entry. Wraps on overflow. Lock-free.
    void Record(Surface s, uint32_t code, uint32_t appid, Action a) noexcept;

    // Append the ring's 64 most-recent entries to
    // <AppData>\\SteaMidra\\lumacore_diag.txt. Returns true on success,
    // false on any I/O failure (silent — never throws, never logs to UI).
    bool Dump(const char* reason) noexcept;

    // Convenience wrapper Detach paths call. Same as Dump("detach").
    void DumpForDetach() noexcept;

} // namespace Diagnostics

#else  // LUMACORE_DIAGNOSTICS_ENABLED

namespace Diagnostics {
    enum class Surface : uint8_t { Callback = 1, EMsgRecv = 2, EMsgSend = 3 };
    enum class Action  : uint8_t { Drop = 1, Forward = 2, Strip = 3, PassThrough = 4 };
    inline void Record(Surface, uint32_t, uint32_t, Action) noexcept {}
    inline bool Dump(const char*) noexcept { return false; }
    inline void DumpForDetach() noexcept {}
}

#endif  // LUMACORE_DIAGNOSTICS_ENABLED
