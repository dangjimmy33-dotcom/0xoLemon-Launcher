// LumaCorePayload — injected into game processes for EOS bridge.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "PayloadPropagator.h"
#include "LcPayloadLogging.h"
#include "RemoteDllDeploy.h"

#include <detours.h>

namespace {
    wchar_t g_selfPath[MAX_PATH] = {};

    using CreateProcessW_t = BOOL(WINAPI*)(LPCWSTR, LPWSTR, LPSECURITY_ATTRIBUTES,
        LPSECURITY_ATTRIBUTES, BOOL, DWORD, LPVOID, LPCWSTR,
        LPSTARTUPINFOW, LPPROCESS_INFORMATION);
    using CreateProcessAsUserW_t = BOOL(WINAPI*)(HANDLE, LPCWSTR, LPWSTR,
        LPSECURITY_ATTRIBUTES, LPSECURITY_ATTRIBUTES, BOOL, DWORD, LPVOID,
        LPCWSTR, LPSTARTUPINFOW, LPPROCESS_INFORMATION);

    CreateProcessW_t       oCreateProcessW       = nullptr;
    CreateProcessAsUserW_t oCreateProcessAsUserW = nullptr;

    BOOL Spawn(HANDLE token, LPCWSTR app, LPWSTR cmd, LPSECURITY_ATTRIBUTES pa,
               LPSECURITY_ATTRIBUTES ta, BOOL inherit, DWORD flags, LPVOID env,
               LPCWSTR cwd, LPSTARTUPINFOW si, LPPROCESS_INFORMATION pi)
    {
        const DWORD spawnFlags = flags | CREATE_SUSPENDED;
        BOOL ok = token
            ? oCreateProcessAsUserW(token, app, cmd, pa, ta, inherit, spawnFlags, env, cwd, si, pi)
            : oCreateProcessW(app, cmd, pa, ta, inherit, spawnFlags, env, cwd, si, pi);
        if (!ok) return ok;

        const bool injected = RemoteInject::LoadDll(pi->hProcess, g_selfPath);
        PayloadLog::Write("propagate pid=" + std::to_string(pi->dwProcessId) +
                          (injected ? " ok" : " FAILED"));

        if (!(flags & CREATE_SUSPENDED)) ResumeThread(pi->hThread);
        return ok;
    }

    BOOL WINAPI hkCreateProcessW(LPCWSTR app, LPWSTR cmd, LPSECURITY_ATTRIBUTES pa,
        LPSECURITY_ATTRIBUTES ta, BOOL inherit, DWORD flags, LPVOID env,
        LPCWSTR cwd, LPSTARTUPINFOW si, LPPROCESS_INFORMATION pi)
    {
        return Spawn(nullptr, app, cmd, pa, ta, inherit, flags, env, cwd, si, pi);
    }

    BOOL WINAPI hkCreateProcessAsUserW(HANDLE token, LPCWSTR app, LPWSTR cmd,
        LPSECURITY_ATTRIBUTES pa, LPSECURITY_ATTRIBUTES ta, BOOL inherit, DWORD flags,
        LPVOID env, LPCWSTR cwd, LPSTARTUPINFOW si, LPPROCESS_INFORMATION pi)
    {
        return Spawn(token, app, cmd, pa, ta, inherit, flags, env, cwd, si, pi);
    }
}

namespace SelfPropagate {
    void Install(HMODULE hSelf) {
        if (!GetModuleFileNameW(hSelf, g_selfPath, MAX_PATH)) return;
        HMODULE k32 = GetModuleHandleW(L"kernel32.dll");
        if (!k32) return;
        oCreateProcessW       = reinterpret_cast<CreateProcessW_t>      (GetProcAddress(k32, "CreateProcessW"));
        oCreateProcessAsUserW = reinterpret_cast<CreateProcessAsUserW_t>(GetProcAddress(k32, "CreateProcessAsUserW"));

        DetourTransactionBegin();
        DetourUpdateThread(GetCurrentThread());
        if (oCreateProcessW)
            DetourAttach(reinterpret_cast<PVOID*>(&oCreateProcessW),
                         reinterpret_cast<PVOID>(hkCreateProcessW));
        if (oCreateProcessAsUserW)
            DetourAttach(reinterpret_cast<PVOID*>(&oCreateProcessAsUserW),
                         reinterpret_cast<PVOID>(hkCreateProcessAsUserW));
        DetourTransactionCommit();
    }
}
