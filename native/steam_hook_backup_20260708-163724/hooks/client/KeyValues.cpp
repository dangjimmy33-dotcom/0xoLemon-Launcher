// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

// KV tree parser hooks. These are observation-only — the actual depot
// patching got moved to ManifestBind::BuildDepotDependency. FindOrCreateKey
// fires hundreds of times per second after the app list loads, so per-call
// logging stays off. Flip the Install log line if you ever need to triage
// a KV regression.

#include "hooks/client/KeyValues.h"
#include "hooks/Macros.h"
#include "core/entry.h"
#include "steam/Structs.h"

namespace {

    LM_HOOK(ReadAsBinary, bool,
                KeyValues* kv, void* buf, int depth,
                bool text, void* syms)
    {
        return oReadAsBinary(kv, buf, depth, text, syms);
    }

    LM_HOOK(FindOrCreateKey, KeyValues*,
                KeyValues* parent, const char* name,
                bool create, KeyValues** outChild)
    {
        return oFindOrCreateKey(parent, name, create, outChild);
    }

}

namespace KVHooks {

    void Install() {
        LM_TX_BEGIN();
        LM_INSTALL(ReadAsBinary);
        LM_INSTALL(FindOrCreateKey);
        LM_TX_COMMIT();
        LOG_KEYVALUECH_INFO("KVHooks::Install: ReadAsBinary={} | FindOrCreateKey={}",
                            oReadAsBinary    ? "live" : "miss",
                            oFindOrCreateKey ? "live" : "miss");
    }

    void Uninstall() {
        LM_TX_BEGIN();
        LM_REMOVE(FindOrCreateKey);
        LM_REMOVE(ReadAsBinary);
        LM_TX_COMMIT();
        LOG_KEYVALUECH_INFO("KVHooks::Uninstall: done");
    }

}
