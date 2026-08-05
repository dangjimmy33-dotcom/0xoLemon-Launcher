// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "hooks/client/PackageInject.h"
#include "hooks/Macros.h"
#include "hooks/capture/RuntimeCapture.h"
#include "core/entry.h"
#include "Steam/Callback.h"
#include "runtime/HookStatus.h"
#include "runtime/Ticket.h"
#include "config/LuaLoader.h"
#include "hooks/ui/SteamUI.h"

#include <atomic>
#include <limits>
#include <mutex>
#include <unordered_set>

namespace {
    using CUtlMemoryGrow_t = void* (*)(CUtlVector<AppId_t>*, int);
    using MarkLicenseAsChanged_t = int64_t (*)(void*, uint32_t, bool);
    using ProcessPendingLicenseUpdates_t = bool (*)(void*);

    CUtlMemoryGrow_t oCUtlMemoryGrow = nullptr;
    MarkLicenseAsChanged_t oMarkLicenseAsChanged = nullptr;
    ProcessPendingLicenseUpdates_t oProcessPendingLicenseUpdates = nullptr;

    void* g_pCUser = nullptr;
    void* g_pCPackageInfo = nullptr;
    PackageInfo* g_pInjectedPackageInfo = nullptr;
    bool g_licenseRefreshPending = false;
    bool g_licenseInitialized = false;

    std::mutex g_stubWarnLock;
    std::unordered_set<AppId_t> g_stubWarnedApps;
    std::unordered_set<AppId_t> g_ownershipPatchLoggedApps;

    constexpr uint32_t kInjectedPackageId = 0;
    constexpr uint64_t kInjectedPkgAccessToken = 10660652434190618804ull;

    bool MarkLicenseAsChangedAndProcessUpdates() {
        if (!g_pCUser || !oMarkLicenseAsChanged || !oProcessPendingLicenseUpdates) {
            LOG_PACKAGE_WARN("MarkLicenseAsChangedAndProcessUpdates: dependencies not ready, skipping");
            return false;
        }
        oMarkLicenseAsChanged(g_pCUser, kInjectedPackageId, true);
        oProcessPendingLicenseUpdates(g_pCUser);
        LOG_PACKAGE_DEBUG("MarkLicenseAsChangedAndProcessUpdates: marked package {} as changed and processed updates", kInjectedPackageId);
        return true;
    }

    void TryProcessPendingLicenseRefresh() {
        if (!g_licenseRefreshPending) return;
        if (MarkLicenseAsChangedAndProcessUpdates())
            g_licenseRefreshPending = false;
    }

    bool InitFakeLicenseOnce(PackageInfo* pPkg) {
        if (!pPkg || !oCUtlMemoryGrow) {
            LOG_PACKAGE_WARN("InitFakeLicenseOnce: missing dependencies");
            return false;
        }

        std::vector<AppId_t> appIds = LuaLoader::GetAllDepotIds();
        if (appIds.empty()) {
            LOG_PACKAGE_DEBUG("InitFakeLicenseOnce: no appIds to inject");
            g_licenseInitialized = true;
            return true;
        }

        oCUtlMemoryGrow(&pPkg->AppIdVec, static_cast<int>(appIds.size()));
        AppId_t* data = pPkg->AppIdVec.m_Memory.m_pMemory;
        uint32_t currentSize = pPkg->AppIdVec.m_Size;
        
        for (AppId_t appId : appIds) {
            data[currentSize++] = appId;
        }
        pPkg->AppIdVec.m_Size = currentSize;

        g_licenseInitialized = true;
        g_licenseRefreshPending = true;
        TryProcessPendingLicenseRefresh();
        
        LOG_PACKAGE_INFO("InitFakeLicenseOnce: injected {} appIds into package {}", appIds.size(), kInjectedPackageId);
        return true;
    }

    LM_HOOK(GetPackageInfo, PackageInfo*, void* pThis, uint32_t packageId, uint64_t accessToken) {
        if (!g_pCPackageInfo) {
            g_pCPackageInfo = pThis;
            LOG_PACKAGE_DEBUG("GetPackageInfo: captured g_pCPackageInfo=0x{:X}", reinterpret_cast<uint64_t>(pThis));
        }
        return oGetPackageInfo(pThis, packageId, accessToken);
    }

    LM_HOOK(CheckAppOwnership, bool, void* pObj, AppId_t appId, AppOwnership* pOwn) {
        if (!g_pCUser) {
            g_pCUser = pObj;
            LOG_PACKAGE_DEBUG("CheckAppOwnership: captured CUser 0x{:X}", reinterpret_cast<uint64_t>(pObj));
        }

        if (!pOwn) return oCheckAppOwnership(pObj, appId, pOwn);

        bool bSteam = oCheckAppOwnership(pObj, appId, pOwn);
        
        PackageInject::TryInitFakeLicenseOnce();

        if (LuaLoader::HasDepot(appId)) {
            if (bSteam && pOwn->ExistInPackageNums > 1 && pOwn->ReleaseState == EAppReleaseState::Released) {
                // Steam says it's genuinely owned. Mark as owned/family shared.
                if (pOwn->bBorrowed || pOwn->bFamilyShared) {
                    LuaLoader::MarkFamilyShared(appId);
                } else {
                    LuaLoader::MarkOwned(appId);
                }
                HookStatus::RecordOwnershipCheck(
                    appId, false, !(pOwn->bBorrowed || pOwn->bFamilyShared), (pOwn->bBorrowed || pOwn->bFamilyShared),
                    static_cast<int32_t>(pOwn->ReleaseState), pOwn->ExistInPackageNums, pOwn->bBorrowed, pOwn->bFamilyShared);
            } else {
                HookStatus::RecordOwnershipCheck(
                    appId, true, false, false,
                    static_cast<int32_t>(pOwn->ReleaseState), pOwn->ExistInPackageNums, pOwn->bBorrowed, pOwn->bFamilyShared);
                
                pOwn->PackageId = kInjectedPackageId;
                pOwn->ReleaseState = EAppReleaseState::Released;
                pOwn->bOwnsLicense = true;
                pOwn->bFreeLicense = false;

                bool firstPatchLog = false;
                {
                    std::lock_guard lock(g_stubWarnLock);
                    firstPatchLog = g_ownershipPatchLoggedApps.insert(appId).second;
                }
                if (firstPatchLog) {
                    LOG_PACKAGE_INFO(
                        "CheckAppOwnership: appId={} patched -> owned (was result={} ExistInPkg={} ReleaseState={} borrowed={} familyShared={})",
                        appId, bSteam, pOwn->ExistInPackageNums,
                        static_cast<int>(pOwn->ReleaseState),
                        pOwn->bBorrowed, pOwn->bFamilyShared);
                } else {
                    LOG_PACKAGE_TRACE("CheckAppOwnership: appId={} patched -> owned (repeat)", appId);
                }

                if (Ticket::IsKnownSteamDrmApp(appId)) {
                    bool first = false;
                    {
                        std::lock_guard lock(g_stubWarnLock);
                        first = g_stubWarnedApps.insert(appId).second;
                    }
                    if (first) {
                        LOG_PACKAGE_INFO("CheckAppOwnership: appId={} is a known Steam Stub title; "
                                         "normal launches use the dedicated SteamStub auto route "
                                         "when the preflight AppTicket is OK. If Steam still "
                                         "reports error 54 after that, try Remove SteamStub from SteaMidra.",
                                         appId);
                    }
                }
                return true;
            }
        }
        return bSteam;
    }

    LM_HOOK(GetSubscribedApps, uint32_t, void* pThis, uint32_t* pAppList, uint32_t size, uint8_t unknownFlag) {
        uint32_t count = oGetSubscribedApps(pThis, pAppList, size, unknownFlag);
        std::vector<AppId_t> roots = LuaLoader::GetLibraryAppIds();
        if (roots.empty()) {
            HookStatus::RecordSubscribedApps(count, roots.size(), 0, count, size);
            return count;
        }

        uint32_t written = 0;
        uint32_t advertisedAdds = 0;
        const bool canScanOriginal = pAppList && count <= size;

        for (AppId_t appId : roots) {
            bool alreadyInList = false;
            if (canScanOriginal) {
                for (uint32_t i = 0; i < count; i++) {
                    if (pAppList[i] == appId) {
                        alreadyInList = true;
                        break;
                    }
                }
            }

            if (alreadyInList) continue;

            advertisedAdds++;
            if (pAppList && count + written < size) {
                pAppList[count + written] = appId;
                written++;
            }
        }

        uint32_t advertisedTotal = count + advertisedAdds;
        if (advertisedTotal < count) {
            advertisedTotal = (std::numeric_limits<uint32_t>::max)();
        }

        LOG_PACKAGE_INFO("GetSubscribedApps: original={}, roots={}, written={}, advertised={}, buffer={}",
                         count, roots.size(), written, advertisedTotal, size);
        HookStatus::RecordSubscribedApps(count, roots.size(), written, advertisedTotal, size);
        return advertisedTotal;
    }

    constexpr int kAchievementCallbackIds[] = {
        UserStatsReceived_t::k_iCallback,
        UserStatsStored_t::k_iCallback,
        UserAchievementStored_t::k_iCallback,
        UserAchievementIconFetched_t::k_iCallback,
    };

    static bool IsAchievementCallback(int iCallback) {
        for (int id : kAchievementCallbackIds)
            if (id == iCallback) return true;
        return false;
    }

    static bool RewriteAchievementCallbackGameId(int iCallback, void* pCallbackData, int cubCallbackData) {
        AppId_t real = SteamCapture::ActiveRouteRealAppId();
        if (real == 0 || real == kOnlineFixAppId) return false;
        if (cubCallbackData < static_cast<int>(sizeof(uint64_t))) return false;
        if (pCallbackData == nullptr) return false;

        auto* pGameId = static_cast<uint64_t*>(pCallbackData);
        AppId_t current = static_cast<AppId_t>(*pGameId & 0xFFFFFF);
        if (current != real) return false;

        *pGameId = (*pGameId & ~static_cast<uint64_t>(0xFFFFFF))
                 | static_cast<uint64_t>(kOnlineFixAppId);
        LOG_ONLINEFIX_DEBUG("achievement callback {} m_nGameID {} -> {}",
                            iCallback, real, kOnlineFixAppId);
        return true;
    }

    LM_HOOK(SendCallbackToPipe, bool, void* pSteamEngine, HSteamPipe hSteamPipe,
              HSteamUser iClientUser, int iCallback, void* pCallbackData, int cubCallbackData) {
        if (iCallback == AppLicensesChanged_t::k_iCallback) {
            auto* p = static_cast<AppLicensesChanged_t*>(pCallbackData);
            LOG_PACKAGE_DEBUG("SendCallbackToPipe: AppLicensesChanged m_bReloadAll={} -> true",
                           p->m_bReloadAll);
            p->m_bReloadAll = true;
            return oSendCallbackToPipe(pSteamEngine, hSteamPipe, iClientUser,
                                       iCallback, pCallbackData, cubCallbackData);
        }

        if (IsAchievementCallback(iCallback)
            && SteamCapture::StatsScopePipe() == hSteamPipe
            && SteamCapture::ActiveRouteRealAppId() != 0)
        {
            const bool firstOk = oSendCallbackToPipe(pSteamEngine, hSteamPipe, iClientUser,
                                                     iCallback, pCallbackData, cubCallbackData);
            if (RewriteAchievementCallbackGameId(iCallback, pCallbackData, cubCallbackData)) {
                LOG_ONLINEFIX_TRACE("OnlineFix_Dual_Dispatch: cb={} pipe=0x{:08X} -> appid {}",
                                    iCallback,
                                    static_cast<uint32_t>(hSteamPipe),
                                    kOnlineFixAppId);
                oSendCallbackToPipe(pSteamEngine, hSteamPipe, iClientUser,
                                    iCallback, pCallbackData, cubCallbackData);
            }
            return firstOk;
        }

        return oSendCallbackToPipe(pSteamEngine, hSteamPipe, iClientUser,
                                   iCallback, pCallbackData, cubCallbackData);
    }
}

namespace PackageInject {
    void Install() {
        LM_BIND(CUtlMemoryGrow);
        LM_BIND(MarkLicenseAsChanged);
        LM_BIND(ProcessPendingLicenseUpdates);

        LM_TX_BEGIN();
        LM_INSTALL(GetPackageInfo);
        LM_INSTALL(CheckAppOwnership);
        LM_INSTALL(GetSubscribedApps);
        LM_INSTALL(SendCallbackToPipe);
        LM_TX_COMMIT();
    }

    void Uninstall() {
        LM_TX_BEGIN();
        LM_REMOVE(GetPackageInfo);
        LM_REMOVE(CheckAppOwnership);
        LM_REMOVE(GetSubscribedApps);
        LM_REMOVE(SendCallbackToPipe);
        LM_TX_COMMIT();
        
        oCUtlMemoryGrow = nullptr;
        oMarkLicenseAsChanged = nullptr;
        oProcessPendingLicenseUpdates = nullptr;
        g_pCUser = nullptr;
        g_pCPackageInfo = nullptr;
        g_pInjectedPackageInfo = nullptr;
        g_licenseInitialized = false;
        g_licenseRefreshPending = false;
    }

    bool TryInitFakeLicenseOnce() {
        if (g_licenseInitialized) return true;
        if (g_pCPackageInfo && oGetPackageInfo) {
            PackageInfo* pPkg = oGetPackageInfo(g_pCPackageInfo, kInjectedPackageId, kInjectedPkgAccessToken);
            if(!pPkg) {
                LOG_PACKAGE_WARN("TryInitFakeLicenseOnce: GetPackageInfo returned null for injected package");
                return false;
            }
            if(!g_pInjectedPackageInfo) g_pInjectedPackageInfo = pPkg;
            return InitFakeLicenseOnce(pPkg);
        }
        return false;
    }

    void NotifyLicenseChanged() {
        if (!oCUtlMemoryGrow) {
            LOG_PACKAGE_WARN("NotifyLicenseChanged: CUtlMemoryGrow not resolved yet, skipping");
            return;
        }
        
        PackageInfo* pPkg = g_pInjectedPackageInfo;
        if (!pPkg && g_pCPackageInfo && oGetPackageInfo) {
            pPkg = oGetPackageInfo(g_pCPackageInfo, kInjectedPackageId, kInjectedPkgAccessToken);
            if (pPkg) g_pInjectedPackageInfo = pPkg;
        }

        if (!pPkg) {
            HookStatus::SetStartupRefreshState("startup-waiting-packageinfo");
            LOG_PACKAGE_WARN("NotifyLicenseChanged: package not captured yet, leaving Lua changes pending");
            return;
        }

        std::vector<AppId_t> removals = LuaLoader::TakePendingRemovals();
        uint32_t removedCount = 0;
        for (AppId_t id : removals) {
            if (pPkg->AppIdVec.FindAndFastRemove(id)) {
                ++removedCount;
                LOG_PACKAGE_DEBUG("NotifyLicenseChanged: removed AppId {} from vector", id);
            }
        }

        std::vector<AppId_t> additions = LuaLoader::TakePendingAdditions();
        if (!additions.empty()) {
            oCUtlMemoryGrow(&pPkg->AppIdVec, static_cast<int>(additions.size()));
            AppId_t* data = pPkg->AppIdVec.m_Memory.m_pMemory;
            uint32_t currentSize = pPkg->AppIdVec.m_Size;
            
            for (AppId_t appId : additions) {
                data[currentSize++] = appId;
            }
            pPkg->AppIdVec.m_Size = currentSize;
        }

        if (additions.empty() && removals.empty()) {
            LOG_PACKAGE_DEBUG("NotifyLicenseChanged: no changes");
            return;
        }

        bool refreshedLicense = false;
        if (g_pCUser && oMarkLicenseAsChanged && oProcessPendingLicenseUpdates) {
            oMarkLicenseAsChanged(g_pCUser, kInjectedPackageId, true);
            oProcessPendingLicenseUpdates(g_pCUser);
            HookStatus::SetPackageState(false, false, false, true);
            refreshedLicense = true;
        } else {
            HookStatus::SetStartupRefreshState("startup-waiting-cuser");
            LOG_PACKAGE_WARN("NotifyLicenseChanged: pCUser not ready, package vector updated locally only");
        }

        std::unordered_set<AppId_t> libraryRoots;
        for (AppId_t id : LuaLoader::GetLibraryAppIds()) {
            libraryRoots.insert(id);
        }
        uint32_t queuedTouches = 0;
        for (AppId_t id : additions) {
            if (libraryRoots.count(id)) {
                SteamUI::CancelLibraryRemoval(id);
                SteamUI::QueueLibraryTouch(id);
                ++queuedTouches;
            }
        }
        uint32_t queuedRemovals = 0;
        for (AppId_t id : removals) {
            SteamUI::QueueLibraryRemoval(id);
            ++queuedRemovals;
        }
        if (queuedTouches || queuedRemovals) {
            LOG_PACKAGE_INFO("NotifyLicenseChanged: {} added, {} removed ({} from vector)", additions.size(), removals.size(), removedCount);
        }
    }

    bool InjectIntoPackage0(PackageInfo* pPkg, const std::vector<AppId_t>& appIds, const char* reason) {
        // Obsolete in PackageInject architecture. Handled inside InitFakeLicenseOnce.
        return true; 
    }

    bool InjectIntoPackage0(const std::vector<AppId_t>& appIds, const char* reason) {
        return true;
    }

    PackageInfo* GetPackage0() {
        return g_pInjectedPackageInfo;
    }
}
