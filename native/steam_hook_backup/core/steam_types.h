#pragma once
#include <cstdint>

// Steam type definitions extracted from LumaCore
using AppId_t = uint32_t;
using HSteamPipe = int32_t;
using HSteamUser = int32_t;

constexpr AppId_t k_uAppIdInvalid = 0x0;

enum class EPackageStatus : uint32_t {
    Available = 0,
    Preorder = 1,
    Unavailable = 2,
    Invalid = 3
};

enum class EAppReleaseState : uint32_t {
    Unknown = 0,
    Unavailable = 1,
    Prerelease = 2,
    PreloadOnly = 3,
    Released = 4
};

// CUtlVector - Valve's dynamic array
template<typename T>
struct CUtlMemory {
    T* m_pMemory;
    int m_nAllocationCount;
    int m_nGrowSize;
};

template<typename T>
struct CUtlVector {
    CUtlMemory<T> m_Memory;
    int m_Size;
    T* m_pElements;
};

// PackageInfo - Steam's internal package structure
struct PackageInfo {
    uint32_t PackageId;
    EPackageStatus Status;
    uint32_t ChangeNumber;
    uint64_t AccessToken;
    CUtlVector<AppId_t> AppIdVec;
    // ... other fields we don't need
};

// AppOwnership - Result from CheckAppOwnership
struct AppOwnership {
    uint32_t PackageId;
    EAppReleaseState ReleaseState;
    bool bFreeLicense;
    bool bOwnsLicense;
    uint32_t ExistInPackageNums;
    // ... other fields
};
