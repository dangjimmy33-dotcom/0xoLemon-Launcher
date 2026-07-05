#include "package_hooks.h"
#include "../lua_loader.h"
#include "../pattern_scanner.h"
#include "../patterns.h"
#include <detours.h>
#include <Windows.h>
#include <algorithm>

namespace OxoHooks {

// Original function pointers
LoadPackage_t Original_LoadPackage = nullptr;
CheckAppOwnership_t Original_CheckAppOwnership = nullptr;
GetSubscribedApps_t Original_GetSubscribedApps = nullptr;
CUtlMemoryGrow_t Original_CUtlMemoryGrow = nullptr;

static bool g_package0Injected = false;

// Hooked LoadPackage - inject our AppIDs into Package 0
bool Hook_LoadPackage(PackageInfo* pInfo, uint8_t* sha1, int32_t cn, void* p4) {
    bool result = Original_LoadPackage(pInfo, sha1, cn, p4);
    
    if (pInfo && pInfo->PackageId == 0 && !g_package0Injected) {
        std::vector<AppId_t> appIds = OxoLua::GetAllDepotIds();
        
        if (!appIds.empty() && Original_CUtlMemoryGrow) {
            uint32_t oldSize = pInfo->AppIdVec.m_Size;
            uint32_t numToAdd = static_cast<uint32_t>(appIds.size());
            
            // Grow vector
            Original_CUtlMemoryGrow(&pInfo->AppIdVec, numToAdd);
            
            // Copy AppIDs
            AppId_t* dst = pInfo->AppIdVec.m_Memory.m_pMemory + oldSize;
            for (uint32_t i = 0; i < numToAdd; i++) {
                *dst++ = appIds[i];
            }
            
            pInfo->AppIdVec.m_Size = oldSize + numToAdd;
            g_package0Injected = true;
            
            char msg[256];
            sprintf_s(msg, "[OxoHook] Injected %d apps into Package 0 (total: %d)\n", 
                     numToAdd, pInfo->AppIdVec.m_Size);
            OutputDebugStringA(msg);
        }
    }
    
    return result;
}

// Hooked CheckAppOwnership - fake ownership for lua-tracked apps
bool Hook_CheckAppOwnership(void* pObj, AppId_t appId, AppOwnership* pOwn) {
    bool result = Original_CheckAppOwnership(pObj, appId, pOwn);
    
    if (pOwn && OxoLua::HasDepot(appId)) {
        // Check if genuinely owned
        if (result && pOwn->ExistInPackageNums > 1 && 
            pOwn->ReleaseState == EAppReleaseState::Released) {
            // Actually owned - mark to exclude from future injection
            OxoLua::MarkOwned(appId);
            return result;
        }
        
        // Fake ownership
        pOwn->PackageId = 0;
        pOwn->ReleaseState = EAppReleaseState::Released;
        pOwn->bFreeLicense = false;
        pOwn->bOwnsLicense = true;
        pOwn->ExistInPackageNums = 1;
        
        char msg[256];
        sprintf_s(msg, "[OxoHook] Faked ownership for AppID %u\n", appId);
        OutputDebugStringA(msg);
        
        return true;
    }
    
    return result;
}

// Hooked GetSubscribedApps - add our apps to library list
uint32_t Hook_GetSubscribedApps(void* pThis, uint32_t* pAppList, uint32_t size, uint8_t unknownFlag) {
    uint32_t count = Original_GetSubscribedApps(pThis, pAppList, size, unknownFlag);
    
    std::vector<AppId_t> roots = OxoLua::GetLibraryAppIds();
    if (roots.empty()) return count;
    
    uint32_t written = 0;
    uint32_t advertisedAdds = 0;
    
    // Check if we have space and avoid duplicates
    for (AppId_t appId : roots) {
        bool alreadyInList = false;
        
        if (pAppList && count <= size) {
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
    
    char msg[256];
    sprintf_s(msg, "[OxoHook] GetSubscribedApps: original=%d, added=%d, total=%d\n", 
             count, written, advertisedTotal);
    OutputDebugStringA(msg);
    
    return advertisedTotal;
}

void InstallPackageHooks() {
    OutputDebugStringA("[OxoHook] ========================================\n");
    OutputDebugStringA("[OxoHook] Starting pattern scanning for Steam functions...\n");
    OutputDebugStringA("[OxoHook] ========================================\n");
    
    // Get steamclient64.dll module
    HMODULE hSteamClient = GetModuleHandleA("steamclient64.dll");
    if (!hSteamClient) {
        OutputDebugStringA("[OxoHook] ERROR: steamclient64.dll not loaded!\n");
        return;
    }
    
    char msg[512];
    sprintf_s(msg, "[OxoHook] steamclient64.dll base: 0x%p\n", (void*)hSteamClient);
    OutputDebugStringA(msg);
    
    // Helper: Try all patterns in a set
    auto ScanWithFallbacks = [&](const char* name, const SteamPatterns::PatternSet& patternSet) -> void* {
        for (int i = 0; i < 4; i++) {
            const char* pattern = SteamPatterns::GetPattern(patternSet, i);
            if (!pattern) continue;
            
            sprintf_s(msg, "[OxoHook] Trying %s pattern v%d: %s...\n", name, i+1, pattern);
            OutputDebugStringA(msg);
            
            auto result = OxoPattern::ScanModule(hSteamClient, pattern);
            if (result.found) {
                sprintf_s(msg, "[OxoHook] ✅ Found %s at 0x%p (pattern v%d)\n", name, result.address, i+1);
                OutputDebugStringA(msg);
                return result.address;
            }
        }
        
        sprintf_s(msg, "[OxoHook] ❌ WARNING: %s not found (tried 4 patterns)\n", name);
        OutputDebugStringA(msg);
        return nullptr;
    };
    
    // Scan for functions with fallback support
    OutputDebugStringA("[OxoHook] ----------------------------------------\n");
    Original_LoadPackage = (LoadPackage_t)ScanWithFallbacks("LoadPackage", SteamPatterns::LoadPackage);
    
    OutputDebugStringA("[OxoHook] ----------------------------------------\n");
    Original_CheckAppOwnership = (CheckAppOwnership_t)ScanWithFallbacks("CheckAppOwnership", SteamPatterns::CheckAppOwnership);
    
    OutputDebugStringA("[OxoHook] ----------------------------------------\n");
    Original_GetSubscribedApps = (GetSubscribedApps_t)ScanWithFallbacks("GetSubscribedApps", SteamPatterns::GetSubscribedApps);
    
    OutputDebugStringA("[OxoHook] ----------------------------------------\n");
    Original_CUtlMemoryGrow = (CUtlMemoryGrow_t)ScanWithFallbacks("CUtlMemoryGrow", SteamPatterns::CUtlMemoryGrow);
    
    OutputDebugStringA("[OxoHook] ========================================\n");
    
    // Install Detours hooks if functions were found
    int hooksInstalled = 0;
    
    if (Original_LoadPackage || Original_CheckAppOwnership || Original_GetSubscribedApps) {
        DetourTransactionBegin();
        DetourUpdateThread(GetCurrentThread());
        
        if (Original_LoadPackage) {
            LONG result = DetourAttach(&(PVOID&)Original_LoadPackage, Hook_LoadPackage);
            if (result == NO_ERROR) {
                hooksInstalled++;
                OutputDebugStringA("[OxoHook] ✅ Hooked LoadPackage\n");
            } else {
                sprintf_s(msg, "[OxoHook] ❌ Failed to hook LoadPackage (error %ld)\n", result);
                OutputDebugStringA(msg);
            }
        }
        
        if (Original_CheckAppOwnership) {
            LONG result = DetourAttach(&(PVOID&)Original_CheckAppOwnership, Hook_CheckAppOwnership);
            if (result == NO_ERROR) {
                hooksInstalled++;
                OutputDebugStringA("[OxoHook] ✅ Hooked CheckAppOwnership\n");
            } else {
                sprintf_s(msg, "[OxoHook] ❌ Failed to hook CheckAppOwnership (error %ld)\n", result);
                OutputDebugStringA(msg);
            }
        }
        
        if (Original_GetSubscribedApps) {
            LONG result = DetourAttach(&(PVOID&)Original_GetSubscribedApps, Hook_GetSubscribedApps);
            if (result == NO_ERROR) {
                hooksInstalled++;
                OutputDebugStringA("[OxoHook] ✅ Hooked GetSubscribedApps\n");
            } else {
                sprintf_s(msg, "[OxoHook] ❌ Failed to hook GetSubscribedApps (error %ld)\n", result);
                OutputDebugStringA(msg);
            }
        }
        
        LONG commitResult = DetourTransactionCommit();
        
        OutputDebugStringA("[OxoHook] ========================================\n");
        if (commitResult == NO_ERROR) {
            sprintf_s(msg, "[OxoHook] ✅ Successfully installed %d hooks!\n", hooksInstalled);
            OutputDebugStringA(msg);
            OutputDebugStringA("[OxoHook] Steam ownership injection is ACTIVE\n");
        } else {
            sprintf_s(msg, "[OxoHook] ❌ ERROR: DetourTransactionCommit failed (error %ld)\n", commitResult);
            OutputDebugStringA(msg);
        }
        OutputDebugStringA("[OxoHook] ========================================\n");
    } else {
        OutputDebugStringA("[OxoHook] ❌ ERROR: No functions found - hooks not installed!\n");
        OutputDebugStringA("[OxoHook] This Steam build may need new patterns.\n");
        OutputDebugStringA("[OxoHook] Please report your Steam version for pattern update.\n");
        OutputDebugStringA("[OxoHook] ========================================\n");
    }
}

void UninstallPackageHooks() {
    if (Original_LoadPackage) {
        DetourTransactionBegin();
        DetourUpdateThread(GetCurrentThread());
        DetourDetach(&(PVOID&)Original_LoadPackage, Hook_LoadPackage);
        DetourDetach(&(PVOID&)Original_CheckAppOwnership, Hook_CheckAppOwnership);
        DetourDetach(&(PVOID&)Original_GetSubscribedApps, Hook_GetSubscribedApps);
        DetourTransactionCommit();
    }
    
    OutputDebugStringA("[OxoHook] Package hooks uninstalled\n");
}

} // namespace OxoHooks
