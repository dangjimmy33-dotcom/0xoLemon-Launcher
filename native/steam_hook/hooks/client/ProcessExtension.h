// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include "hooks/client/PipeWatch.h"

namespace ProcessExtension {
    void Reset();
    void QueueLaunchHint(const char* exePath, AppId_t appId);
    void OnGamePipe(const PipeWatch::ProcessSnapshot& snapshot);
}
