// _0xoLemonCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "core/Orchestrator.h"
#include "hooks/client/DepotKeys.h"
#include "hooks/client/DecryptionKeyHook.h"
#include "hooks/client/IPCBus.h"
#include "hooks/client/ManifestBind.h"
#include "patterns/PatternFetcher.h"
#include "hooks/capture/SteamCapture.h"
#include "hooks/ui/SteamUI.h"
#include "hooks/client/PacketRouter.h"
#include "hooks/client/PackagePatch.h"
#include "hooks/client/LicenseHooks.h"
#include "hooks/client/OnlineFixInject.h"
#include "runtime/Diagnostics.h"
#include "sffcore/NativeCore.h"
#include "runtime/Logger.h"


namespace _0xoLemonCore {

    using HookOp = void(*)();
    static constexpr HookOp kUninstallOrder[] = {
        DepotKeys::Uninstall,
        DecryptionKeyHook::Uninstall,
        IPCBus::Uninstall,
        ManifestBind::Uninstall,
        SteamCapture::Uninstall,
        SteamUI::CoreUnhook,
        PacketRouter::Uninstall,
        OnlineFixInject::Uninstall,
        PackagePatch::Uninstall,
        LicenseHooks::Uninstall,
    };

    void Attach(bool compatibilityReady) {
        if (!SffCore::Initialize())
            LOG_WARN("Native SFF core initialization failed; continuing with hook-only runtime");

        // Compatibility-sensitive IPC handlers must never run with stale method
        // hashes. Unknown Steam builds stay in pass-through mode until both the
        // per-build pattern TOML and IPC metadata are available. This prevents a
        // half-installed hook set from changing Steam's native entitlement/UI state.
        DepotKeys::Install();
        DecryptionKeyHook::Install();
        if (compatibilityReady) {
            IPCBus::Install();
        } else {
            LOG_WARN("Compatibility degraded: IPCBus disabled; Steam remains pass-through");
        }
        ManifestBind::Install();
        PacketRouter::Install();
        OnlineFixInject::Install();
        LicenseHooks::Install();
    }

    void Detach() {
#ifdef OXOLEMONCORE_DIAGNOSTICS_ENABLED
        Diagnostics::DumpForDetach();
#endif
        for (auto fn : kUninstallOrder) fn();
        PatternFetcher::Reset();
        SffCore::Shutdown();
    }
}
