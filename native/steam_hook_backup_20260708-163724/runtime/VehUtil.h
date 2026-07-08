// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include <windows.h>
#include <cstdint>
#include <vector>
#include "core/entry.h"
#include "patterns/PatternFetcher.h"
#include "hooks/SigTypes.h"
#include "hooks/ui/StatusWriter.h"
#include "hooks/client/StringFind.h"
#include "runtime/Logger.h"

// ── VEH one-shot capture entry ───────────────────────────────────────────────
struct CaptureEntry {
    void**      funcPtr;      // &o##Name
    void**      outPtr;       // capture target (e.g. &g_pCUser)
    uint8_t     restoreByte;  // original first byte, saved before arm
    const char* label;
};

// ── X-macro helpers (all include trailing semicolons for list expansion) ─────
// CAPTURE_LIST(X): X(FuncName, CaptureVar)
#define VEH_DECL_CAPTURE(name, out) name##_t o##name; void* out;
#define VEH_ARM(name, out)          ARM_CAPTURE_D(name, out);
// LOCATE_LIST(X): X(FuncName)
#define VEH_DECL_RESOLVE(name)      name##_t o##name;
#define VEH_LOCATE(name)            LM_BIND(name);
#define VEH_ZERO_RESOLVE(name)      o##name = nullptr;

// ── ARM_CAPTURE_D ────────────────────────────────────────────────────────────
// TOML-only resolver. Finds the function via PatternFetcher, saves original
// byte, pushes to g_captures, arms int3. Requires g_captures
// (std::vector<CaptureEntry>) in scope.
#define ARM_CAPTURE_D(name, outVar)                                            \
    do {                                                                        \
        void* _p_ = PatternFetcher::Resolve(diversion_hModule, #name);          \
        if (_p_) {                                                              \
            LOG_DEBUG("Capture: {} armed via TOML @ 0x{:X}", #name,             \
                      reinterpret_cast<uintptr_t>(_p_));                        \
            o##name = reinterpret_cast<name##_t>(_p_);                          \
            g_captures.push_back({                                              \
                reinterpret_cast<void**>(&o##name),                             \
                reinterpret_cast<void**>(&(outVar)),                            \
                *reinterpret_cast<uint8_t*>(_p_),                               \
                #name                                                           \
            });                                                                 \
            VehUtil::ArmInt3(_p_);                                              \
            StatusWriter::RecordHit(#name);                                     \
        } else {                                                                \
            LOG_WARN("Capture: {} FAILED - TOML entry missing", #name);         \
            StatusWriter::RecordMiss(#name);                                    \
        }                                                                       \
    } while (0)

// ── ARM_CAPTURE_STR_D ───────────────────────────────────────────────────────
// Try string xref first (most update-proof anchor), fall back to PatternFetcher.
// Use when the target function carries a unique debug string in its body.
#define ARM_CAPTURE_STR_D(name, outVar, strSigs)                               \
    do {                                                                        \
        void* _p_ = nullptr;                                                    \
        const char* _matched_str_ = nullptr;                                    \
        for (const auto& _s_ : (strSigs)) {                                     \
            _p_ = StringFind::FindFunction(diversion_hModule,                   \
                                           _s_.str, _s_.occurrence);             \
            if (_p_) { _matched_str_ = _s_.str; break; }                       \
        }                                                                       \
        if (_p_) {                                                              \
            LOG_DEBUG("Capture: {} armed via string-xref \"{}\" @ 0x{:X}",      \
                      #name, _matched_str_, reinterpret_cast<uintptr_t>(_p_));  \
        } else {                                                                \
            _p_ = PatternFetcher::Resolve(diversion_hModule, #name);            \
            if (_p_) {                                                          \
                LOG_DEBUG("Capture: {} armed via TOML (str-xref missed) @ 0x{:X}", \
                          #name, reinterpret_cast<uintptr_t>(_p_));             \
            } else {                                                            \
                LOG_WARN("Capture: {} FAILED - both string-xref and TOML missed", \
                         #name);                                                \
            }                                                                   \
        }                                                                       \
        if (_p_) {                                                              \
            o##name = reinterpret_cast<name##_t>(_p_);                          \
            g_captures.push_back({                                              \
                reinterpret_cast<void**>(&o##name),                             \
                reinterpret_cast<void**>(&(outVar)),                            \
                *reinterpret_cast<uint8_t*>(_p_),                               \
                #name                                                           \
            });                                                                 \
            VehUtil::ArmInt3(_p_);                                              \
            StatusWriter::RecordHit(#name);                                     \
        } else {                                                                \
            StatusWriter::RecordMiss(#name);                                    \
        }                                                                       \
    } while (0)

// ── VEH_CLEANUP_CAPTURES ─────────────────────────────────────────────────────
// Restore unarmed int3 sites, zero all pointers, clear the table.
#define VEH_CLEANUP_CAPTURES(captures)                                         \
    do {                                                                        \
        for (auto& _cap_ : (captures)) {                                       \
            if (*_cap_.funcPtr                                                  \
                && *reinterpret_cast<uint8_t*>(*_cap_.funcPtr) == 0xCC)         \
                VehUtil::RestoreByte(*_cap_.funcPtr, _cap_.restoreByte);        \
            *_cap_.funcPtr = nullptr;                                           \
            *_cap_.outPtr  = nullptr;                                           \
        }                                                                       \
        (captures).clear();                                                     \
    } while (0)

namespace VehUtil {
    void ArmInt3(void* target);
    void RestoreByte(void* target, uint8_t original);
}
