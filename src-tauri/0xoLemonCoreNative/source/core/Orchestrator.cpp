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
    static constexpr HookOp kInstallOrder[] = {
        DepotKeys::Install,
        DecryptionKeyHook::Install,
        IPCBus::Install,
        ManifestBind::Install,
        PacketRouter::Install,
        OnlineFixInject::Install,
        LicenseHooks::Install,
    };
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

    void Attach() {
        if (!SffCore::Initialize())
            LOG_WARN("Native SFF core initialization failed; continuing with hook-only runtime");
        for (auto fn : kInstallOrder) fn();
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
