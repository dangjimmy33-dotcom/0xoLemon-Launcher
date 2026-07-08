// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

// Hook plumbing macros for LumaCore. Every address resolution flows through
// ByteSearch, which reads the per-build TOML published to the pattern repo.
// When the running Steam build has no TOML, hooks fail clean: ByteSearch
// returns nullptr, the macro logs a miss into status.json via
// HookStatus::RecordMissed, and the install pass keeps going.
//
// All LM_INSTALL / LM_BIND targets resolve against diversion_hModule (the
// hooked copy of steamclient64.dll). SteamUI hooks call ByteSearch directly
// against hSteamUI rather than going through these macros.
//
// LM_INSTALL_STR / LM_BIND_STR variants iterate a StringXRefSig array of TOML
// lookup-key candidates and use the first key that resolves.

#include <windows.h>
#include <cstddef>

#include <detours.h>

#include "hooks/SigTypes.h"
#include "patterns/ByteScan.h"
#include "runtime/HookStatus.h"
#include "runtime/Logger.h"

// diversion_hModule is declared in core/entry.h. Call sites include entry.h
// before reaching these macros; not pulling it here keeps Macros.h out of
// any circular include chain with PatternFetcher and StatusWriter.

// Open a Detours transaction. DetourTransactionBegin starts the batch;
// DetourUpdateThread registers the calling thread so Detours adjusts its
// instruction pointer past any trampolines before Commit fires.
// Always pair with LM_TX_COMMIT.
#define LM_TX_BEGIN()                         \
    do {                                       \
        DetourTransactionBegin();              \
        DetourUpdateThread(GetCurrentThread())

// Close and apply the open Detours transaction atomically.
#define LM_TX_COMMIT()                        \
        DetourTransactionCommit();             \
    } while (0)

// Declare a hooked function and its original-pointer trampoline.
// Expands to:
//   1. typedef ret(__fastcall* fn##_t)(args);
//   2. inline fn##_t o##fn = nullptr;            (trampoline slot)
//   3. ret __fastcall hk##fn(args)               (hook signature; body in braces)
// Call o##fn(...) inside the hook body to invoke the original.
#define LM_HOOK(fn, ret, ...)                              \
    typedef ret(__fastcall* fn##_t)(__VA_ARGS__);           \
    inline fn##_t o##fn = nullptr;                          \
    ret __fastcall hk##fn(__VA_ARGS__)

// Resolve `fn` against the steamclient TOML via ByteSearch and attach a
// Detours hook. On miss, log + RecordMissed and skip the install.
// Call inside LM_TX_BEGIN / LM_TX_COMMIT.
#define LM_INSTALL(fn)                                                          \
    do {                                                                         \
        void* _p_ = ByteSearch(diversion_hModule, #fn);                          \
        if (_p_) {                                                               \
            LOG_DEBUG("Hook: {} attached @ 0x{:X}",                              \
                      #fn, reinterpret_cast<uintptr_t>(_p_));                    \
            o##fn = reinterpret_cast<fn##_t>(_p_);                               \
            DetourAttach(reinterpret_cast<PVOID*>(&o##fn),                       \
                         reinterpret_cast<PVOID>(hk##fn));                       \
            HookStatus::RecordInstalled();                                       \
        } else {                                                                 \
            LOG_WARN("Hook: {} skipped (TOML entry missing for current build)",  \
                     #fn);                                                       \
            HookStatus::RecordMissed(#fn);                                       \
        }                                                                        \
    } while (0)

// Resolve `fn` by trying every lookup key in `arr` (a StringXRefSig array
// of length `n`) until ByteSearch returns non-null. Each entry's `.name`
// field carries a candidate TOML key. On miss, log + RecordMissed and skip.
// Call inside LM_TX_BEGIN / LM_TX_COMMIT.
#define LM_INSTALL_STR(fn, arr, n)                                                \
    do {                                                                          \
        void* _p_ = nullptr;                                                      \
        const char* _matched_ = nullptr;                                          \
        for (std::size_t _i_ = 0; _i_ < (n) && !_p_; ++_i_) {                     \
            _p_ = ByteSearch(diversion_hModule, (arr)[_i_].name);                 \
            if (_p_) _matched_ = (arr)[_i_].name;                                 \
        }                                                                         \
        if (_p_) {                                                                \
            LOG_DEBUG("Hook: {} attached via TOML key \"{}\" @ 0x{:X}",           \
                      #fn, _matched_, reinterpret_cast<uintptr_t>(_p_));          \
            o##fn = reinterpret_cast<fn##_t>(_p_);                                \
            DetourAttach(reinterpret_cast<PVOID*>(&o##fn),                        \
                         reinterpret_cast<PVOID>(hk##fn));                        \
            HookStatus::RecordInstalled();                                        \
        } else {                                                                  \
            LOG_WARN("Hook: {} skipped (no TOML key in array resolved)",          \
                     #fn);                                                        \
            HookStatus::RecordMissed(#fn);                                        \
        }                                                                         \
    } while (0)

// Resolve a function address into o##fn without hooking it. Used to call
// internal Steam functions directly. No Detours transaction needed.
#define LM_BIND(fn)                                                              \
    do {                                                                          \
        void* _p_ = ByteSearch(diversion_hModule, #fn);                           \
        o##fn = reinterpret_cast<fn##_t>(_p_);                                    \
        if (_p_) {                                                                \
            LOG_DEBUG("Resolve: {} bound @ 0x{:X}",                               \
                      #fn, reinterpret_cast<uintptr_t>(_p_));                     \
            HookStatus::RecordInstalled();                                        \
        } else {                                                                  \
            LOG_WARN("Resolve: {} skipped (TOML entry missing)", #fn);            \
            HookStatus::RecordMissed(#fn);                                        \
        }                                                                         \
    } while (0)

// Resolve into o##fn by trying every lookup key in `arr` (size `n`) until
// ByteSearch returns non-null. No hook installed. On miss, log + RecordMissed.
#define LM_BIND_STR(fn, arr, n)                                                   \
    do {                                                                          \
        void* _p_ = nullptr;                                                      \
        const char* _matched_ = nullptr;                                          \
        for (std::size_t _i_ = 0; _i_ < (n) && !_p_; ++_i_) {                     \
            _p_ = ByteSearch(diversion_hModule, (arr)[_i_].name);                 \
            if (_p_) _matched_ = (arr)[_i_].name;                                 \
        }                                                                         \
        o##fn = reinterpret_cast<fn##_t>(_p_);                                    \
        if (_p_) {                                                                \
            LOG_DEBUG("Resolve: {} bound via TOML key \"{}\" @ 0x{:X}",           \
                      #fn, _matched_, reinterpret_cast<uintptr_t>(_p_));          \
            HookStatus::RecordInstalled();                                        \
        } else {                                                                  \
            LOG_WARN("Resolve: {} skipped (no TOML key in array resolved)",       \
                     #fn);                                                        \
            HookStatus::RecordMissed(#fn);                                        \
        }                                                                         \
    } while (0)

// Remove a Detours hook and clear the trampoline slot. Safe to call even
// when the install pass earlier skipped the hook (o##fn stays nullptr).
// Call inside LM_TX_BEGIN / LM_TX_COMMIT.
#define LM_REMOVE(fn)                                                            \
    do {                                                                          \
        if (o##fn) {                                                              \
            DetourDetach(reinterpret_cast<PVOID*>(&o##fn),                        \
                         reinterpret_cast<PVOID>(hk##fn));                        \
            o##fn = nullptr;                                                      \
        }                                                                         \
    } while (0)

// RuntimeCapture variant. Resolve `fn` by trying every lookup key in `arr`
// (size `n`), save the original first byte, push the entry to g_captures, and
// arm an int3 at the resolved address so the next call into the function
// triggers the VEH that snapshots `outVar`. Requires g_captures
// (std::vector<CaptureEntry>) and VehUtil::ArmInt3 in scope at the call site.
// On miss, log + RecordMissed and leave outVar untouched.
#define LM_CAPTURE(fn, outVar, arr, n)                                             \
    do {                                                                          \
        void* _p_ = nullptr;                                                      \
        const char* _matched_ = nullptr;                                          \
        for (std::size_t _i_ = 0; _i_ < (n) && !_p_; ++_i_) {                     \
            _p_ = ByteSearch(diversion_hModule, (arr)[_i_].name);                 \
            if (_p_) _matched_ = (arr)[_i_].name;                                 \
        }                                                                         \
        if (_p_) {                                                                \
            LOG_DEBUG("Capture: {} armed via TOML key \"{}\" @ 0x{:X}",           \
                      #fn, _matched_, reinterpret_cast<uintptr_t>(_p_));          \
            o##fn = reinterpret_cast<fn##_t>(_p_);                                \
            g_captures.push_back({                                                \
                reinterpret_cast<void**>(&o##fn),                                 \
                reinterpret_cast<void**>(&(outVar)),                              \
                *reinterpret_cast<uint8_t*>(_p_),                                 \
                #fn                                                               \
            });                                                                   \
            VehUtil::ArmInt3(_p_);                                                \
            HookStatus::RecordInstalled();                                        \
        } else {                                                                  \
            LOG_WARN("Capture: {} skipped (no TOML key in array resolved)",       \
                     #fn);                                                        \
            HookStatus::RecordMissed(#fn);                                        \
        }                                                                         \
    } while (0)
