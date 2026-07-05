#pragma once
#include "../steam_types.h"

namespace OxoHooks {

    // Install Detours hooks
    void InstallPackageHooks();
    
    // Uninstall hooks
    void UninstallPackageHooks();

    // Hook function signatures (these will be resolved via pattern scanning later)
    // For now, we'll use simplified versions

    // LoadPackage - called when Steam loads package data
    typedef bool (*LoadPackage_t)(PackageInfo* pInfo, uint8_t* sha1, int32_t cn, void* p4);
    extern LoadPackage_t Original_LoadPackage;

    // CheckAppOwnership - called when Steam checks if user owns an app
    typedef bool (*CheckAppOwnership_t)(void* pObj, AppId_t appId, AppOwnership* pOwn);
    extern CheckAppOwnership_t Original_CheckAppOwnership;

    // GetSubscribedApps - returns list of owned apps for library
    typedef uint32_t (*GetSubscribedApps_t)(void* pThis, uint32_t* pAppList, uint32_t size, uint8_t unknownFlag);
    extern GetSubscribedApps_t Original_GetSubscribedApps;

    // CUtlMemoryGrow - Valve's vector growth function
    typedef void* (*CUtlMemoryGrow_t)(void* pVec, int grow_size);
    extern CUtlMemoryGrow_t Original_CUtlMemoryGrow;

} // namespace OxoHooks
