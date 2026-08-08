#pragma once

#include <chrono>
#include <filesystem>
#include <string>
#include <string_view>

namespace SffCore::Process {
struct LaunchResult {
    bool success = false;
    std::string message;
};

bool IsRunning(std::wstring_view imageName);
bool IsElevated();
bool KillByName(std::wstring_view imageName);
bool WaitForExit(std::wstring_view imageName, std::chrono::milliseconds timeout);
LaunchResult LaunchSteamUnelevated(const std::filesystem::path& steamExe,
                                   const std::filesystem::path& cwd = {});
} // namespace SffCore::Process
