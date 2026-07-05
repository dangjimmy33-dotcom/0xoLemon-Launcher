// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "LibraryInjector.h"
#include "core/entry.h"
#include "Logger.h"
#include "config/Settings.h"

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <filesystem>
#include <mutex>
#include <unordered_set>

namespace Injection {

    namespace {

        std::mutex g_mtx;
        std::unordered_set<uint32_t> g_injected;

        bool WasInjected(uint32_t pid) {
            std::scoped_lock lock(g_mtx);
            return g_injected.count(pid) > 0;
        }

        void MarkInjected(uint32_t pid) {
            std::scoped_lock lock(g_mtx);
            g_injected.insert(pid);
        }

        // Determine if the process is 64-bit by checking if it's running under WOW64
        bool IsProcess64Bit(uint32_t pid) {
            HANDLE hProcess = OpenProcess(PROCESS_QUERY_INFORMATION, FALSE, pid);
            if (!hProcess) return true; // assume 64-bit on error

            BOOL wow64 = FALSE;
            if (!IsWow64Process(hProcess, &wow64)) {
                CloseHandle(hProcess);
                return true;
            }
            CloseHandle(hProcess);
            return !wow64;
        }

        // Resolve the library path relative to the Steam install dir
        std::wstring ResolvePath(const std::string& configured) {
            if (configured.empty()) return {};

            std::filesystem::path p(configured);
            if (p.is_absolute()) {
                // Already absolute; convert to wstring
                auto& s = p.native();
                return std::wstring(s.begin(), s.end());
            }

            // Relative to Steam install dir
            std::filesystem::path base(SteamInstallPath);
            std::wstring result = (base / p).wstring();
            return result;
        }

        enum class InjectResult { Ok, Fail, Skipped };

        InjectResult InjectDll(uint32_t pid, const std::wstring& dllPath) {
            if (dllPath.empty()) return InjectResult::Skipped;
            if (!std::filesystem::exists(dllPath)) {
                LOG_WARN("Injection: library not found path={}", std::string(dllPath.begin(), dllPath.end()));
                return InjectResult::Fail;
            }

            HANDLE hProcess = OpenProcess(
                PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION |
                PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ,
                FALSE, pid);
            if (!hProcess) {
                LOG_WARN("Injection: failed to open pid={} err={}", pid, GetLastError());
                return InjectResult::Fail;
            }

            // Allocate memory in the target process for the DLL path
            size_t pathSize = (dllPath.size() + 1) * sizeof(wchar_t);
            void* remoteMem = VirtualAllocEx(hProcess, nullptr, pathSize,
                                              MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
            if (!remoteMem) {
                LOG_WARN("Injection: VirtualAllocEx failed pid={} err={}", pid, GetLastError());
                CloseHandle(hProcess);
                return InjectResult::Fail;
            }

            if (!WriteProcessMemory(hProcess, remoteMem, dllPath.c_str(),
                                     pathSize, nullptr)) {
                LOG_WARN("Injection: WriteProcessMemory failed pid={} err={}", pid, GetLastError());
                VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
                CloseHandle(hProcess);
                return InjectResult::Fail;
            }

            // Create a remote thread that calls LoadLibraryW
            HMODULE kernel32 = GetModuleHandleA("kernel32.dll");
            if (!kernel32) {
                VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
                CloseHandle(hProcess);
                return InjectResult::Fail;
            }

            auto loadLibW = reinterpret_cast<LPTHREAD_START_ROUTINE>(
                GetProcAddress(kernel32, "LoadLibraryW"));
            if (!loadLibW) {
                VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
                CloseHandle(hProcess);
                return InjectResult::Fail;
            }

            HANDLE hThread = CreateRemoteThread(hProcess, nullptr, 0,
                                                  loadLibW, remoteMem, 0, nullptr);
            if (!hThread) {
                LOG_WARN("Injection: CreateRemoteThread failed pid={} err={}", pid, GetLastError());
                VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
                CloseHandle(hProcess);
                return InjectResult::Fail;
            }

            WaitForSingleObject(hThread, 30000);
            CloseHandle(hThread);
            VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
            CloseHandle(hProcess);

            LOG_MISC_INFO("Injection: injected into pid={} path={}",
                           pid, std::string(dllPath.begin(), dllPath.end()));
            return InjectResult::Ok;
        }
    }

    Settings LoadSettings() {
        Settings s;
        s.enabled = ::Settings::processExtensionEnabled;
        s.libraryX86 = ::Settings::processExtensionX86;
        s.libraryX64 = ::Settings::processExtensionX64;
        return s;
    }

    void Apply(uint32_t pid) {
        if (pid == 0) return;

        Settings cfg = LoadSettings();
        if (!cfg.enabled) return;

        if (WasInjected(pid)) return;

        bool is64Bit = IsProcess64Bit(pid);
        const std::string& libPath = is64Bit ? cfg.libraryX64 : cfg.libraryX86;
        if (libPath.empty()) return;

        std::wstring resolved = ResolvePath(libPath);
        auto result = InjectDll(pid, resolved);
        if (result == InjectResult::Ok) {
            MarkInjected(pid);
        }
    }

}
