// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "hooks/capture/RuntimeCapture.h"
#include "hooks/Macros.h"
#include "hooks/client/SteamStubAuto.h"
#include "hooks/ui/SteamUI.h"
#include "runtime/VehUtil.h"
#include "runtime/HookStatus.h"
#include "runtime/ProtectionProbe.h"
#include "runtime/Ticket.h"
#include "hooks/client/OnlineFixInject.h"
#include "core/entry.h"

#include <algorithm>
#include <chrono>
#include <string_view>
#include <thread>
#include <unordered_set>

namespace {
    // ── function type aliases (alphabetical) ─────────────────────────────────
    using BuildSpawnEnvBlock_t           = __int64(*)(void*, uint64_t*, void*, void*, uint64_t*, void*, int, void*, void*, unsigned int, char);
    using CUtlBufferEnsureCapacity_t     = void*(*)(CUtlBuffer*, int);
    using CUtlMemoryGrow_t               = void*(*)(CUtlVector<AppId_t>*, int);
    using GetAppDataFromAppInfo_t        = int64(*)(void*, AppId_t, const char*, uint8*, int32);
    using GetAppIDForCurrentPipe_t       = AppId_t(*)(void*);
    using GetPackageInfo_t               = PackageInfo*(*)(void*, uint32, int64);
    using MarkLicenseAsChanged_t         = int64(*)(void*, uint32, bool);
    using ProcessPendingLicenseUpdates_t = bool(*)(void*);

    // ── X-macro lists ────────────────────────────────────────────────────────
    // One-shot int3: on hit, ctx->Rcx stored to the named output variable.
    #define VEH_GRAB_LIST(X)                         \
        X(GetAppDataFromAppInfo,  g_pCAppInfoCache)

    // Resolve-only (no int3).
    #define VEH_TRACK_LIST(X)            \
        X(CUtlBufferEnsureCapacity)      \
        X(CUtlMemoryGrow)               \
        X(ProcessPendingLicenseUpdates)

    // ── generated declarations ───────────────────────────────────────────────
    VEH_GRAB_LIST(VEH_DECL_CAPTURE)
    VEH_TRACK_LIST(VEH_DECL_RESOLVE)

    // ── Detours-captured pointers (set on first call, never cleared) ─────────
    // Detours hooks fire on every call regardless of when they
    // were installed, so we capture on the first
    // natural Steam call after login — even if that happens after startup.
    void* g_pCUser        = nullptr;
    void* g_pCPackageInfo = nullptr;

    // ── per-session state ─────────────────────────────────────────────────────
    void*                 g_steamEngine        = nullptr;
    uint8_t*              g_spawnProcessTarget = nullptr;
    PVOID                 g_vehHandle          = nullptr;
    std::atomic<AppId_t>  g_OnlineFixRealAppId{0};
    std::atomic<uint32>   g_OnlineFixRouteMode{static_cast<uint32>(SteamCapture::OnlineFixRouteMode::None)};
    // Pipe-scoped fine gate that pairs with the thread-local depth counter
    // below. Stamped by EnterStatsScope on entry to an IClientUserStats IPC,
    // cleared by LeaveStatsScope on exit. The achievement-callback rewrite
    // path reads this under acquire
    // ordering so cross-pipe bleed cannot land a rewrite on a pipe that did
    // not originate the user-stats call.
    std::atomic<HSteamPipe> g_StatsScopePipe{0};
    // Scoped real-appid override depth for IClientUserStats traffic.
    // Thread-local so concurrent IPC pipes from worker threads don't bleed
    // into each other. Incremented by SetUserStatsContext(true), decremented
    // by SetUserStatsContext(false). Read by the GetAppIDForCurrentPipe
    // detour to decide whether to return the real appid.
    thread_local uint32   g_userStatsAppIdOverrideDepth = 0;
    std::unordered_map<AppId_t, std::string> g_GameNameCache;
    static std::vector<CaptureEntry> g_captures;

    SteamCapture::OnlineFixRouteMode CurrentOnlineFixMode() {
        return static_cast<SteamCapture::OnlineFixRouteMode>(
            g_OnlineFixRouteMode.load(std::memory_order_acquire));
    }

    void SetOnlineFixRoute(AppId_t realAppId, SteamCapture::OnlineFixRouteMode mode) {
        g_OnlineFixRealAppId.store(realAppId, std::memory_order_release);
        g_OnlineFixRouteMode.store(static_cast<uint32>(mode), std::memory_order_release);
    }

    AppId_t ActiveRouteRealAppIdInternal() {
        if (SteamStubAuto::IsActive())
            return SteamStubAuto::RealAppId();
        return g_OnlineFixRealAppId.load(std::memory_order_acquire);
    }

    // ── GetAppIDForCurrentPipe Detours hook ───────────────────────────────────
    // Captures g_steamEngine (RCX = this) on first call and applies the scoped
    // real-appid override for IClientUserStats traffic.
    //
    // The override returns the real appid only when ALL of:
    //   1. SetUserStatsContext(true) is currently on the stack on this thread
    //      (g_userStatsAppIdOverrideDepth > 0)
    //   2. A route is active for this session (manual OnlineFix or SteamStub)
    //   3. The engine itself reports the Spacewar masquerade (appid == 480)
    //
    // Every other call path returns the engine's value untouched. That keeps
    // the lobby / friends / controller / RemoteStorage paths byte-identical
    // to the existing 480 behaviour. The depth counter is thread-local so
    // concurrent IPC pipes don't bleed into each other.
    LM_HOOK(GetAppIDForCurrentPipe, AppId_t, void* pEngine) {
        if (g_steamEngine == nullptr && pEngine != nullptr) {
            g_steamEngine = pEngine;
            LOG_MISC_INFO("Captured g_steamEngine: 0x{:X}",
                          reinterpret_cast<uint64_t>(pEngine));
        }
        AppId_t appid = oGetAppIDForCurrentPipe(pEngine);
        if (g_userStatsAppIdOverrideDepth > 0
            && ActiveRouteRealAppIdInternal() != 0
            && appid == kOnlineFixAppId) {
            AppId_t real = ActiveRouteRealAppIdInternal();
            LOG_MISC_TRACE("GetAppIDForCurrentPipe: stats-scope override {} -> {}",
                           appid, real);
            return real;
        }
        return appid;
    }

    // ── BuildSpawnEnvBlock Detours hook ──────────────────────────────────────
    // Manual -onlinefix keeps the old overlay trick. Dedicated SteamStub auto
    // keeps CGameID as 480 for Steam tracking, and exposes only overlay identity
    // to the real app.
    LM_HOOK(BuildSpawnEnvBlock, __int64,
            void* pThis, uint64_t* pCGameID, void* a3, void* env,
            uint64_t* pOverlayCGameID, void* a6, int a7,
            void* a8, void* a9, unsigned int a10, char a11)
    {
        SteamCapture::OnlineFixRouteMode mode = CurrentOnlineFixMode();
        AppId_t onlineFixRealAppId = g_OnlineFixRealAppId.load(std::memory_order_acquire);
        AppId_t steamStubRealAppId = SteamStubAuto::RealAppId();
        AppId_t realAppId = steamStubRealAppId ? steamStubRealAppId : onlineFixRealAppId;
        AppId_t overlayAppId = pOverlayCGameID
            ? static_cast<AppId_t>(*pOverlayCGameID & 0xFFFFFF) : 0;
        AppId_t cgameAppId = pCGameID
            ? static_cast<AppId_t>(*pCGameID & 0xFFFFFF) : 0;

        uint64_t prevCGame = pCGameID ? *pCGameID : 0;
        uint64_t prevOverlay = pOverlayCGameID ? *pOverlayCGameID : 0;
        bool patchedOverlay = false;

        LOG_MISC_INFO("BuildSpawnEnvBlock: input routeMode={} pThis=0x{:X} env=0x{:X} pCGameID=0x{:X} rawCGame={:#x} pOverlay=0x{:X} rawOverlay={:#x} realAppId={}",
                      SteamCapture::OnlineFixRouteModeName(mode),
                      reinterpret_cast<uint64_t>(pThis),
                      reinterpret_cast<uint64_t>(env),
                      reinterpret_cast<uint64_t>(pCGameID),
                      prevCGame,
                      reinterpret_cast<uint64_t>(pOverlayCGameID),
                      prevOverlay,
                      realAppId);

        if (realAppId
            && (mode != SteamCapture::OnlineFixRouteMode::None || SteamStubAuto::IsActive())
            && pOverlayCGameID
            && overlayAppId == kOnlineFixAppId) {
            *pOverlayCGameID = (prevOverlay & ~static_cast<uint64_t>(0xFFFFFF))
                             | static_cast<uint64_t>(realAppId);
            patchedOverlay = true;
        }

        if (SteamStubAuto::IsActive() && steamStubRealAppId) {
            LOG_MISC_INFO("BuildSpawnEnvBlock: steamstub-auto dedicated cgame kept {:#x}->{:#x} overlay {:#x}->{:#x} realAppId={} env=0x{:X}",
                          prevCGame, pCGameID ? *pCGameID : 0,
                          prevOverlay, pOverlayCGameID ? *pOverlayCGameID : 0,
                          steamStubRealAppId,
                          reinterpret_cast<uint64_t>(env));
        } else if (patchedOverlay) {
            LOG_MISC_INFO("BuildSpawnEnvBlock: routeMode={} cgame kept {:#x}->{:#x} overlay {:#x}->{:#x} realAppId={} env=0x{:X}",
                          SteamCapture::OnlineFixRouteModeName(mode),
                          prevCGame, pCGameID ? *pCGameID : 0,
                          prevOverlay, pOverlayCGameID ? *pOverlayCGameID : 0,
                          realAppId,
                          reinterpret_cast<uint64_t>(env));
        } else {
            LOG_MISC_TRACE("BuildSpawnEnvBlock: routeMode={} cgame={} overlay={} realAppId={} env=0x{:X} (no patch)",
                           SteamCapture::OnlineFixRouteModeName(mode),
                           cgameAppId, overlayAppId, realAppId,
                           reinterpret_cast<uint64_t>(env));
        }
        return oBuildSpawnEnvBlock(pThis, pCGameID, a3, env,
                                    pOverlayCGameID, a6, a7, a8, a9, a10, a11);
    }

    // Prevents substring matches like "-onlinefixpatch" triggering the -onlinefix path.
    static bool HasExactFlag(const char* cmd, const char* flag) {
        const char* p = cmd;
        size_t n = strlen(flag);
        while ((p = strstr(p, flag))) {
            bool startOk = (p == cmd || p[-1] == ' ');
            bool endOk   = (p[n] == '\0' || p[n] == ' ');
            if (startOk && endOk) return true;
            p += n;
        }
        return false;
    }

    // ── VEH handler ──────────────────────────────────────────────────────────
    // Scoped to this module's int3 sites only. Foreign RIP ->
    // EXCEPTION_CONTINUE_SEARCH so other VEH handlers still get their turn.
    LONG CALLBACK VehHandler(PEXCEPTION_POINTERS pExInfo) {
        PCONTEXT ctx = pExInfo->ContextRecord;

        if (pExInfo->ExceptionRecord->ExceptionCode == EXCEPTION_BREAKPOINT) {
            for (auto& cap : g_captures) {
                if (*cap.funcPtr && ctx->Rip == reinterpret_cast<uint64_t>(*cap.funcPtr)) {
                    *cap.outPtr = reinterpret_cast<void*>(ctx->Rcx);
                    *reinterpret_cast<uint8_t*>(*cap.funcPtr) = cap.restoreByte;
                    LOG_MISC_INFO("Captured {}: 0x{:X}", cap.label,
                                  reinterpret_cast<uint64_t>(*cap.outPtr));
                    return EXCEPTION_CONTINUE_EXECUTION;
                }
            }

            // CUser_SpawnProcess(pCUser, pExePath, pCommandLine, pWorkingDir,
            //                   pGameID, ...)
            // RCX=pCUser, RDX=pExePath, R8=pCommandLine, R9=pWorkingDir
            // [RSP+0x28]=pGameID (5th arg, pointer to CGameID, low 24 bits = AppId)
            if (g_spawnProcessTarget
                && ctx->Rip == reinterpret_cast<uint64_t>(g_spawnProcessTarget)) {
                auto* pGameID = reinterpret_cast<uint64_t*>(
                    *reinterpret_cast<uint64_t*>(ctx->Rsp + 0x28));
                const char* exePath = reinterpret_cast<const char*>(ctx->Rdx);
                const char* cmdLine = reinterpret_cast<const char*>(ctx->R8);
                const char* workDir = reinterpret_cast<const char*>(ctx->R9);

                if (!pGameID) {
                    LOG_MISC_WARN("SpawnProcess: pGameID is null, exe=\"{}\" cmd=\"{}\"",
                                  exePath ? exePath : "(null)",
                                  cmdLine ? cmdLine : "(null)");
                    *g_spawnProcessTarget = 0x48;
                    ctx->EFlags |= 0x100;
                    return EXCEPTION_CONTINUE_EXECUTION;
                }
                AppId_t appId = static_cast<AppId_t>(*pGameID & 0xFFFFFF);

                *g_spawnProcessTarget = 0x48;
                ctx->EFlags |= 0x100;

                bool hasDepot = LuaLoader::HasDepot(appId);
                bool owned = LuaLoader::IsOwned(appId);
                bool hasFlag = (cmdLine != nullptr) && HasExactFlag(cmdLine, "-onlinefix");
                bool knownSteamStub = Ticket::IsKnownSteamDrmApp(appId);
                ProtectionProbe::ScanResult steamStubProbe{};
                if (hasDepot && !owned && !knownSteamStub && exePath) {
                    steamStubProbe = ProtectionProbe::ScanBeforeSpawn(appId, exePath);
                }
                const bool probeSteamStub = steamStubProbe.valid && steamStubProbe.routeAccepted;
                const bool detectedSteamStub = knownSteamStub || probeSteamStub;
                const std::string steamStubSource = knownSteamStub ? "known-list" :
                    (steamStubProbe.valid && steamStubProbe.detected ? "pre-spawn-probe" : "none");
                const std::string steamStubMethod = knownSteamStub ? "known-list" :
                    (steamStubProbe.valid ? steamStubProbe.method : "skipped");
                const std::string steamStubImage = steamStubProbe.valid && steamStubProbe.detected
                    ? steamStubProbe.imagePath
                    : (exePath ? exePath : "");
                bool autoSteamStubCandidate = hasDepot && !owned && !hasFlag && detectedSteamStub;
                HookStatus::RecordSteamStubDetection(appId, steamStubSource, steamStubMethod,
                                                     steamStubImage, steamStubProbe.candidates,
                                                     detectedSteamStub,
                                                     knownSteamStub ? "accepted" : steamStubProbe.routeReason);

                LOG_MISC_INFO("SpawnProcess: hit appid={} hasDepot={} owned={} hasFlag={} autoSteamStubCandidate={} steamStubRouteAccepted={} steamStubRouteReason={} steamStubSource={} steamStubMethod={} steamStubImage=\"{}\" candidates={} exe=\"{}\" cmd=\"{}\"",
                              appId, hasDepot, owned, hasFlag, autoSteamStubCandidate,
                              detectedSteamStub,
                              knownSteamStub ? "accepted" : steamStubProbe.routeReason,
                              steamStubSource,
                              steamStubMethod,
                              steamStubImage,
                              steamStubProbe.candidates,
                              exePath ? exePath : "(null)",
                              cmdLine ? cmdLine : "(null)");

                Ticket::TicketPreflightResult ticketPreflight{};
                if (hasDepot) {
                    ticketPreflight = Ticket::EnsureRegistryTicketsForApp(appId, detectedSteamStub);
                    LOG_MISC_INFO("SpawnProcess: ticketPreflight={} ticketStatus={} ticketSource={} sourceAppId={} changed={} knownSteamStub={} steamStubSource={} steamStubMethod={}",
                                  Ticket::TicketPreflightActionName(ticketPreflight.action),
                                  Ticket::AppTicketStatusName(ticketPreflight.ticketStatus),
                                  Ticket::TicketPreflightSourceName(ticketPreflight.ticketSource),
                                  ticketPreflight.sourceAppId,
                                  ticketPreflight.changed,
                                  ticketPreflight.knownSteamStub,
                                  steamStubSource,
                                  steamStubMethod);
                }

                bool steamStubAuto = SteamStubAuto::ShouldActivate(appId, hasDepot, owned,
                                                                   hasFlag, detectedSteamStub);
                bool missingSteamStubTicket =
                    autoSteamStubCandidate
                    && ticketPreflight.ticketSource == Ticket::TicketPreflightSource::Missing;
                bool routeThrough480 = hasDepot && hasFlag;
                const char* routeReason = hasFlag ? "manual-flag" :
                                          (steamStubAuto ? "steamstub-auto" :
                                           (missingSteamStubTicket ? "steamstub-ticket-missing" : "none"));
                auto routeMode = hasFlag ? SteamCapture::OnlineFixRouteMode::ManualFlag
                                          : SteamCapture::OnlineFixRouteMode::None;

                if (routeThrough480) {
                    SetOnlineFixRoute(appId, routeMode);
                    *pGameID = kOnlineFixAppId;
                    LOG_MISC_INFO("SpawnProcess: 480 route active reason={} routeMode={} appid {} -> {}, real stored",
                                  routeReason, SteamCapture::OnlineFixRouteModeName(routeMode),
                                  appId, kOnlineFixAppId);
                    SteamStubAuto::Clear();
                    OnlineFixInject::QueueInjection(exePath, appId);
                } else if (steamStubAuto) {
                    SetOnlineFixRoute(0, SteamCapture::OnlineFixRouteMode::None);
                    SteamStubAuto::Arm(appId, exePath, probeSteamStub ? steamStubProbe.imagePath : "");
                    *pGameID = kOnlineFixAppId;
                    LOG_MISC_INFO("SpawnProcess: SteamStubAuto active reason={} appid {} -> {}, CGameID stays 480, overlay resolves real ticketSource={} sourceAppId={} steamStubSource={} steamStubMethod={} matchedImage=\"{}\"",
                                  routeReason, appId, kOnlineFixAppId,
                                  Ticket::TicketPreflightSourceName(ticketPreflight.ticketSource),
                                  ticketPreflight.sourceAppId,
                                  steamStubSource,
                                  steamStubMethod,
                                  probeSteamStub ? steamStubProbe.imagePath : "");
                    if (missingSteamStubTicket) {
                        LOG_MISC_WARN("SpawnProcess: SteamStubAuto ticket source missing for appid={}, route still active",
                                      appId);
                    }
                } else {
                    SetOnlineFixRoute(0, SteamCapture::OnlineFixRouteMode::None);
                    SteamStubAuto::Clear();
                    if (missingSteamStubTicket) {
                        LOG_MISC_WARN("SpawnProcess: steamstub-ticket-missing appid={} ticketPreflight={} ticketStatus={} ticketSource={}",
                                      appId,
                                      Ticket::TicketPreflightActionName(ticketPreflight.action),
                                      Ticket::AppTicketStatusName(ticketPreflight.ticketStatus),
                                      Ticket::TicketPreflightSourceName(ticketPreflight.ticketSource));
                    }
                    LOG_MISC_DEBUG("SpawnProcess: 480 route not activated for appid={} "
                                   "(reason: {}{}{}{})",
                                   appId,
                                   !hasDepot ? "no-depot " : "",
                                   owned ? "owned " : "",
                                   !hasFlag && !autoSteamStubCandidate ? "no-flag " : "",
                                   routeThrough480 ? "(internal)" : "");
                }
                return EXCEPTION_CONTINUE_EXECUTION;
            }
        }

        if (pExInfo->ExceptionRecord->ExceptionCode == EXCEPTION_SINGLE_STEP) {
            if (g_spawnProcessTarget
                && ctx->Rip == reinterpret_cast<uint64_t>(g_spawnProcessTarget + 5)) {
                *g_spawnProcessTarget = 0xCC;
                return EXCEPTION_CONTINUE_EXECUTION;
            }
        }

        return EXCEPTION_CONTINUE_SEARCH;
    }
}

namespace SteamCapture {
    void Install() {
        if (g_vehHandle) return;

        VEH_TRACK_LIST(VEH_LOCATE)

        // GetAppDataFromAppInfo: lives in CAppInfoCache. The xref is the
        // unique "name_localized/%s" literal the analyzer pinned the function
        // by. LM_CAPTURE resolves the hook through the per-build TOML; the
        // xref is only kept as informational metadata for log readers and
        // for future TOML re-publishing.
        {
            static constexpr StringXRefSig kAppDataStrSigs[] = {
                {"GetAppDataFromAppInfo", "name_localized/%s"},
            };
            LM_CAPTURE(GetAppDataFromAppInfo, g_pCAppInfoCache,
                       kAppDataStrSigs, std::size(kAppDataStrSigs));
        }

        if (auto* _sp_ = ByteSearch(diversion_hModule, "SpawnProcess")) {
            g_spawnProcessTarget = static_cast<uint8_t*>(_sp_);
            VehUtil::ArmInt3(_sp_);
        }

        if (!g_captures.empty() || g_spawnProcessTarget)
            g_vehHandle = AddVectoredExceptionHandler(1, VehHandler);

        // GetAppIDForCurrentPipe is detoured so it can apply the scoped
        // real-appid override for IClientUserStats traffic and capture the
        // engine pointer inline on the first natural call.
        LM_TX_BEGIN();
        LM_INSTALL(GetAppIDForCurrentPipe);
        LM_INSTALL(BuildSpawnEnvBlock);
        LM_TX_COMMIT();
    }

    void Uninstall() {
        if (g_vehHandle) {
            RemoveVectoredExceptionHandler(g_vehHandle);
            g_vehHandle = nullptr;
        }

        VEH_CLEANUP_CAPTURES(g_captures);

        if (g_spawnProcessTarget && *g_spawnProcessTarget == 0xCC)
            VehUtil::RestoreByte(g_spawnProcessTarget, 0x48);
        g_spawnProcessTarget = nullptr;

        LM_TX_BEGIN();
        LM_REMOVE(GetAppIDForCurrentPipe);
        LM_REMOVE(BuildSpawnEnvBlock);
        LM_TX_COMMIT();

        VEH_TRACK_LIST(VEH_ZERO_RESOLVE)
        SetOnlineFixRoute(0, OnlineFixRouteMode::None);
        SteamStubAuto::Clear();
        g_StatsScopePipe.store(0, std::memory_order_relaxed);
        g_userStatsAppIdOverrideDepth = 0;
        g_steamEngine   = nullptr;
        g_GameNameCache.clear();
        g_pCUser        = nullptr;
        g_pCPackageInfo = nullptr;
    }

    AppId_t GetAppIDForCurrentPipe() {
        if (!g_steamEngine || !oGetAppIDForCurrentPipe) {
            LOG_MISC_WARN("GetAppIDForCurrentPipe called before capture — returning 0");
            return 0;
        }
        auto appid = oGetAppIDForCurrentPipe(g_steamEngine);
        if (!appid) {
            LOG_MISC_TRACE("GetAppIDForCurrentPipe: AppId=0(Not GamePipe)");
        } else {
            LOG_MISC_TRACE("GetAppIDForCurrentPipe: AppId={}", appid);
        }
        return appid;
    }

    AppId_t ResolveAppId() {
        AppId_t routed = ActiveRouteRealAppIdInternal();
        if (routed) return routed;
        return GetAppIDForCurrentPipe();
    }

    AppId_t ActiveRouteRealAppId() {
        return ActiveRouteRealAppIdInternal();
    }

    AppId_t OnlineFixRealAppId() {
        return g_OnlineFixRealAppId.load(std::memory_order_acquire);
    }

    OnlineFixRouteMode OnlineFixMode() {
        return CurrentOnlineFixMode();
    }

    bool OnlineFixRouteIsSteamStubAuto() {
        return SteamStubAuto::IsActive();
    }

    const char* OnlineFixRouteModeName(OnlineFixRouteMode mode) {
        switch (mode) {
        case OnlineFixRouteMode::None:          return "none";
        case OnlineFixRouteMode::ManualFlag:    return "manual-flag";
        }
        return "unknown";
    }

    void SetUserStatsContext(bool active) {
        if (active) {
            ++g_userStatsAppIdOverrideDepth;
        } else if (g_userStatsAppIdOverrideDepth > 0) {
            --g_userStatsAppIdOverrideDepth;
        } else {
            LOG_MISC_WARN("SetUserStatsContext(false) called with depth=0; clamping");
        }
    }

    void EnterStatsScope(HSteamPipe pipe) {
        g_StatsScopePipe.store(pipe, std::memory_order_release);
    }

    void LeaveStatsScope() {
        g_StatsScopePipe.store(0, std::memory_order_release);
    }

    HSteamPipe StatsScopePipe() {
        return g_StatsScopePipe.load(std::memory_order_acquire);
    }

    void EnsureBufferSize(CUtlBuffer* pWrite, int32 size)
    {
        if (oCUtlBufferEnsureCapacity) {
            LOG_MISC_DEBUG("Before ensuring CUtlBuffer capacity: {}", pWrite->DebugString());
            oCUtlBufferEnsureCapacity(pWrite, size);
            LOG_MISC_DEBUG("After ensuring CUtlBuffer capacity: {}", pWrite->DebugString());
        }
        pWrite->m_Put = size;
    }

    // ── Game name ────────────────────────────────────────────────
    std::string GetGameNameByAppID(AppId_t appId)
    {
        auto& entry = g_GameNameCache.try_emplace(appId).first->second;
        if (!entry.empty()) return entry;

        if (g_pCAppInfoCache && oGetAppDataFromAppInfo) {
            char buf[256] = {};
            int64 len = oGetAppDataFromAppInfo(
                g_pCAppInfoCache, appId, "common/name",
                reinterpret_cast<uint8*>(buf), sizeof(buf));
            if (len > 1)
                entry.assign(buf, static_cast<size_t>(len - 1));
        }

        LOG_MISC_DEBUG("GetGameNameByAppID({}): {}", appId, entry);
        return entry;
    }
}
