// LumaCorePayload — injected into game processes for EOS bridge.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "EpicOnlineBridge.h"
#include "LcPayloadLogging.h"
#include "PayloadPropagator.h"

#include <windows.h>
#include <psapi.h>
#include <string>

namespace {

    struct UNICODE_STRING_ { USHORT Length, MaximumLength; PWSTR Buffer; };
    struct LDR_DLL_NOTIF {
        ULONG  Flags;
        const UNICODE_STRING_* FullDllName;
        const UNICODE_STRING_* BaseDllName;
        PVOID  DllBase;
        ULONG  SizeOfImage;
    };
    using LdrNotifyFn   = VOID(CALLBACK*)(ULONG, const LDR_DLL_NOTIF*, PVOID);
    using LdrRegisterFn = LONG(NTAPI*)(ULONG, LdrNotifyFn, PVOID, PVOID*);
    constexpr ULONG LDR_LOADED = 1;
    constexpr wchar_t kEosName[] = L"EOSSDK-Win64-Shipping.dll";

    void TryInstall(HMODULE m) {
        wchar_t base[MAX_PATH] = {};
        if (!GetModuleBaseNameW(GetCurrentProcess(), m, base, MAX_PATH)) return;
        if (_wcsicmp(base, kEosName) == 0) EosBridge::InstallOn(m);
    }

    VOID CALLBACK OnDllLoad(ULONG reason, const LDR_DLL_NOTIF* d, PVOID) {
        if (reason != LDR_LOADED || !d || !d->BaseDllName) return;
        const size_t chars = d->BaseDllName->Length / sizeof(wchar_t);
        if (chars >= MAX_PATH) return;
        wchar_t buf[MAX_PATH];
        memcpy(buf, d->BaseDllName->Buffer, d->BaseDllName->Length);
        buf[chars] = L'\0';
        if (_wcsicmp(buf, kEosName) == 0)
            EosBridge::InstallOn(reinterpret_cast<HMODULE>(d->DllBase));
    }

    void SubscribeToDllLoads() {
        HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
        if (!ntdll) return;
        auto reg = reinterpret_cast<LdrRegisterFn>(
            GetProcAddress(ntdll, "LdrRegisterDllNotification"));
        if (!reg) return;
        PVOID cookie = nullptr;
        reg(0, OnDllLoad, nullptr, &cookie);
    }

    void ScanLoadedModules() {
        HMODULE mods[1024];
        DWORD needed = 0;
        if (!EnumProcessModules(GetCurrentProcess(), mods, sizeof(mods), &needed)) return;
        for (DWORD i = 0; i < needed / sizeof(HMODULE); ++i) TryInstall(mods[i]);
    }

    DWORD WINAPI PayloadMain(LPVOID hSelf) {
        PayloadLog::Init(static_cast<HMODULE>(hSelf));
        PayloadLog::Write("payload attached");
        SelfPropagate::Install(static_cast<HMODULE>(hSelf));
        SubscribeToDllLoads();
        ScanLoadedModules();
        return 0;
    }
}

BOOL APIENTRY DllMain(HMODULE hModule, DWORD reason, LPVOID) {
    if (reason == DLL_PROCESS_ATTACH) {
        DisableThreadLibraryCalls(hModule);
        if (HANDLE h = CreateThread(nullptr, 0, PayloadMain, hModule, 0, nullptr))
            CloseHandle(h);
    }
    return TRUE;
}
