#pragma once

#include <cstdint>
#include <map>
#include <string>

namespace SffCore {

enum class AppState : uint32_t {
    Invalid = 0,
    Uninstalled = 1,
    UpdateRequired = 2,
    FullyInstalled = 4,
    Encrypted = 8,
    Locked = 16,
    FilesMissing = 32,
    AppRunning = 64,
    FilesCorrupt = 128,
    UpdateRunning = 256,
    UpdatePaused = 512,
    UpdateStarted = 1024,
    Uninstalling = 2048,
    BackupRunning = 4096,
    Reconfiguring = 65536,
    Validating = 131072,
    AddingFiles = 262144,
    Preallocating = 524288,
    Downloading = 1048576,
    Staging = 2097152,
    Committing = 4194304,
    UpdateStopping = 8388608,
    Reserved1 = 16777216,
    Reserved2 = 33554432,
};

constexpr bool HasState(uint32_t flags, AppState state) noexcept {
    return (flags & static_cast<uint32_t>(state)) != 0;
}

struct AcfInfo {
    uint32_t appId = 0;
    std::string name;
    uint32_t stateFlags = 0;
    std::string installDir;
    std::map<std::string, std::string, std::less<>> mountedDepots;

    bool NeedsUpdate() const noexcept { return HasState(stateFlags, AppState::UpdateRequired); }
};

} // namespace SffCore
