// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include <vector>
#include "steam/Structs.h"

namespace PackageInject {
    void Install();
    void Uninstall();

    void NotifyLicenseChanged();
    bool TryInitFakeLicenseOnce();
    
    // Inject the provided appIds into the given package.
    bool InjectIntoPackage0(PackageInfo* pPkg, const std::vector<AppId_t>& appIds, const char* reason = nullptr);
    bool InjectIntoPackage0(const std::vector<AppId_t>& appIds, const char* reason = nullptr);
    
    PackageInfo* GetPackage0();
}
