// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.
//
// RSA-PSS-SHA256 signature verification for pattern TOML bodies. The
// fetcher feeds every downloaded TOML body and its matching .sig file
// through Verify() before accepting the entries into the runtime hook
// table. The public key is hardcoded into LumaCore so a hostile mirror
// (or someone serving a forged TOML) cannot forge a sig that passes
// without also obtaining the maintainer's private key.
//
// This module is fully self-contained: BCrypt only, no SteaMidra calls,
// no analyzer calls, no external state. The pattern repo can ship with
// or without sidecar .sig files and LumaCore handles both.
//
// During the rollout window (no .sig files published yet), callers
// should keep Settings::patternRequireSigned=false (the default) and
// missing/invalid signatures degrade to a logged warning. Once every
// shipped TOML carries a sidecar sig, flip require_signed=true to make
// the rejection fatal.

#ifndef LUMACORE_PATTERN_SIG_H
#define LUMACORE_PATTERN_SIG_H

#include <string>
#include <string_view>

namespace PatternSig {

    enum class Result {
        Ok,                 // signature present, validates against the embedded key
        Missing,            // no .sig file (sigBody empty)
        InvalidShape,       // .sig body unparseable (not 256 hex chars)
        BadSignature,       // hex parsed, RSA-PSS-SHA256 verify failed
        KeyUnavailable,     // embedded public key is the placeholder zeros, refuse
        SystemError,        // BCrypt API failure (bug, not an attack)
    };

    // Verify the SHA-256-of-`body` signature carried in `sigBody`. The
    // .sig file format is exactly 256 lowercase hex chars (one 2048-bit
    // RSA-PSS signature, no whitespace). Pass an empty sigBody to signal
    // "no signature was published" so the caller can branch on Missing.
    Result Verify(std::string_view body, std::string_view sigBody);

    // Human-readable label for log lines.
    const char* ResultToStr(Result r);
}

#endif
