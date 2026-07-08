// LumaCore — Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once
#include "core/entry.h"
#include <vector>

namespace PackagePatch {
    // LoadPackage + CheckAppOwnership — patches the package store so that
    // user-supplied depots appear owned and accessible.
    void Install();
    void Uninstall();

    // Inject app IDs directly into the saved package 0 pointer.
    // Returns false if package 0 hasn't been captured yet.
    bool InjectIntoPackage0(const std::vector<AppId_t>& appIds);

    // Returns the saved PackageInfo* for package 0 (nullptr if not yet captured).
    PackageInfo* GetPackage0();

    // Feeds g_pPackage0 from an external source (RuntimeCapture's GetPackageInfo
    // hook) when the LoadPackage hook fired before the hook layer was installed.
    // No-op if package 0 is already known.
    void SetPackage0IfUnknown(PackageInfo* pPkg);
}
