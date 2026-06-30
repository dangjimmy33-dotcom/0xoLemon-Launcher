// ============================================================
// 007Launcher In-Game Overlay — Core Header
// ============================================================
#pragma once

#define WIN32_LEAN_AND_MEAN
#include <Windows.h>
#include <d3d11.h>
#include <dxgi.h>
#include <atomic>
#include <string>

namespace Overlay
{
    // ── Lifecycle ──────────────────────────────────────────
    // Called once from the worker thread spawned in DllMain.
    void Initialise();
    // Called from DLL_PROCESS_DETACH — unhooks everything.
    void Shutdown();

    // ── State ──────────────────────────────────────────────
    // True while the overlay UI layer is visible and consuming input.
    bool IsActive();
    // Toggle visibility (bound to Shift+Tab by default).
    void Toggle();

    // ── Globals shared across modules ──────────────────────
    inline std::atomic<bool>  g_initialised{ false };
    inline std::atomic<bool>  g_active{ false };
    inline std::atomic<bool>  g_shutdown{ false };

    // Name used for IPC objects — derived from the injecting
    // launcher's PID so multiple launchers don't collide.
    inline std::wstring       g_ipcName;

    // DXGI / D3D11 device pointers captured during hooking.
    inline ID3D11Device*        g_device       = nullptr;
    inline ID3D11DeviceContext*  g_context      = nullptr;
    inline IDXGISwapChain*       g_swapChain    = nullptr;
}
