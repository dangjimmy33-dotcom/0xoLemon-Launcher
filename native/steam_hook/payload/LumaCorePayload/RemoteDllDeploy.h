// LumaCorePayload — injected into game processes for EOS bridge.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include <windows.h>

namespace RemoteInject {

    inline bool LoadDll(HANDLE proc, LPCWSTR dllPath) {
        auto loadLib = reinterpret_cast<LPTHREAD_START_ROUTINE>(
            GetProcAddress(GetModuleHandleW(L"kernel32.dll"), "LoadLibraryW"));
        if (!loadLib) return false;

        const SIZE_T bytes = (wcslen(dllPath) + 1) * sizeof(wchar_t);
        void* mem = VirtualAllocEx(proc, nullptr, bytes,
            MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        if (!mem) return false;

        bool ok = false;
        if (WriteProcessMemory(proc, mem, dllPath, bytes, nullptr)) {
            if (HANDLE t = CreateRemoteThread(proc, nullptr, 0, loadLib, mem, 0, nullptr)) {
                ok = (WaitForSingleObject(t, 5000) == WAIT_OBJECT_0);
                CloseHandle(t);
            }
        }
        VirtualFreeEx(proc, mem, 0, MEM_RELEASE);
        return ok;
    }

}
