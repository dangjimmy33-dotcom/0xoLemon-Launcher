#include "sffcore/CoreProcess.h"

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <tlhelp32.h>
#include <shellapi.h>
#endif

#include <algorithm>
#include <cwchar>
#include <thread>
#include <vector>

namespace SffCore::Process {
#ifdef _WIN32
namespace {
bool EqualsInsensitive(std::wstring_view a, std::wstring_view b) {
    if (a.size() != b.size()) return false;
    return _wcsnicmp(a.data(), b.data(), a.size()) == 0;
}

std::vector<DWORD> FindPids(std::wstring_view imageName) {
    std::vector<DWORD> result;
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    if (snap == INVALID_HANDLE_VALUE) return result;
    PROCESSENTRY32W pe{};
    pe.dwSize = sizeof(pe);
    if (Process32FirstW(snap, &pe)) {
        do {
            if (EqualsInsensitive(pe.szExeFile, imageName)) result.push_back(pe.th32ProcessID);
        } while (Process32NextW(snap, &pe));
    }
    CloseHandle(snap);
    return result;
}

} // namespace
#endif

bool IsRunning(std::wstring_view imageName) {
#ifdef _WIN32
    return !FindPids(imageName).empty();
#else
    (void)imageName;
    return false;
#endif
}

bool IsElevated() {
#ifdef _WIN32
    HANDLE token = nullptr;
    if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token)) return false;
    TOKEN_ELEVATION elevation{};
    DWORD size = 0;
    const BOOL ok = GetTokenInformation(token, TokenElevation, &elevation, sizeof(elevation), &size);
    CloseHandle(token);
    return ok && elevation.TokenIsElevated != 0;
#else
    return false;
#endif
}

bool KillByName(std::wstring_view imageName) {
#ifdef _WIN32
    bool any = false;
    bool allOk = true;
    for (DWORD pid : FindPids(imageName)) {
        any = true;
        HANDLE process = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, FALSE, pid);
        if (!process) { allOk = false; continue; }
        if (!TerminateProcess(process, 0)) allOk = false;
        else WaitForSingleObject(process, 5000);
        CloseHandle(process);
    }
    return !any || allOk;
#else
    (void)imageName;
    return false;
#endif
}

bool WaitForExit(std::wstring_view imageName, std::chrono::milliseconds timeout) {
    const auto deadline = std::chrono::steady_clock::now() + timeout;
    while (std::chrono::steady_clock::now() < deadline) {
        if (!IsRunning(imageName)) return true;
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }
    return !IsRunning(imageName);
}

LaunchResult LaunchSteamUnelevated(const std::filesystem::path& steamExe,
                                   const std::filesystem::path& cwd) {
#ifdef _WIN32
    std::error_code ec;
    if (!std::filesystem::exists(steamExe, ec)) return {false, "steam.exe not found"};
    const auto working = cwd.empty() ? steamExe.parent_path() : cwd;

    if (!IsElevated()) {
        std::wstring cmd = L"\"" + steamExe.wstring() + L"\"";
        STARTUPINFOW si{};
        si.cb = sizeof(si);
        PROCESS_INFORMATION pi{};
        if (CreateProcessW(nullptr, cmd.data(), nullptr, nullptr, FALSE, CREATE_NO_WINDOW,
                           nullptr, working.wstring().c_str(), &si, &pi)) {
            CloseHandle(pi.hThread);
            CloseHandle(pi.hProcess);
            return {true, "Steam launched"};
        }
    }

    wchar_t windowsDir[MAX_PATH]{};
    if (GetWindowsDirectoryW(windowsDir, MAX_PATH)) {
        auto explorer = std::filesystem::path(windowsDir) / L"explorer.exe";
        std::wstring cmd = L"\"" + explorer.wstring() + L"\" \"" + steamExe.wstring() + L"\"";
        STARTUPINFOW si{};
        si.cb = sizeof(si);
        PROCESS_INFORMATION pi{};
        if (CreateProcessW(nullptr, cmd.data(), nullptr, nullptr, FALSE, CREATE_NO_WINDOW,
                           nullptr, working.wstring().c_str(), &si, &pi)) {
            CloseHandle(pi.hThread);
            CloseHandle(pi.hProcess);
            return {true, "Steam launched via Explorer"};
        }
    }

    const auto code = reinterpret_cast<INT_PTR>(ShellExecuteW(nullptr, L"open", steamExe.wstring().c_str(),
                                                               nullptr, working.wstring().c_str(), SW_SHOWNORMAL));
    if (code > 32) return {true, "Steam launched via ShellExecute"};
    return {false, "Steam launch failed (ShellExecute=" + std::to_string(code) + ")"};
#else
    (void)steamExe; (void)cwd;
    return {false, "Windows only"};
#endif
}

} // namespace SffCore::Process
