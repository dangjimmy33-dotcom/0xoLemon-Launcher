// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

// Runtime IPC method spec loader. Fetches a TOML from the same KoriaPolis
// pattern repo (path: pattern/ipc/steamclient/<sha256>.toml) that maps
// interface method names to their current FNV-1a hash, fencepost offset,
// and argument count. When the spec is available, IPCBus uses the hash
// from the TOML instead of the compile-time HASH_* constant — this keeps
// IPC dispatch working across Steam client updates that shift method
// order in the vtable.
//
// Fallback: when no TOML exists (first run, offline, repo doesn't ship
// IPC specs yet), IsLoaded() returns false and IPCBus keeps using the
// hardcoded hashes. No functionality is lost.
namespace IpcSpecLoader {

    struct MethodSpec {
        std::string name;
        uint32_t    funcHash   = 0;
        uint32_t    fencepost  = 0;
        uint32_t    argc       = 0;
    };

    struct InterfaceSpec {
        std::string            name;
        uint32_t               interfaceId = 0;
        std::vector<MethodSpec> methods;
    };

    // Load IPC specs for the steamclient module. Hashes diversion_hModule
    // on disk, tries local cache, falls back to the network chain. Safe
    // to call multiple times — subsequent calls are no-ops when already
    // loaded. Must be called from InitThread after PatternFetcher finishes
    // its steamclient leg so the cache dir exists.
    void Load();

    // Resolve the funcHash for a qualified name like "IClientUser::GetSteamID".
    // Returns nullopt when the spec is not loaded or the method is absent.
    std::optional<uint32_t> ResolveHash(const char* qualifiedName);

    // Returns true when a spec TOML was loaded and parsed successfully.
    bool IsLoaded();

    // Drop all loaded specs. Called from LumaCore::Detach.
    void Reset();

} // namespace IpcSpecLoader
