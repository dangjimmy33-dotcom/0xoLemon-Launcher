// ============================================================
// 007Launcher In-Game Overlay — Core Implementation
// ============================================================
// Orchestrates hook installation, IPC setup, and teardown.
// ============================================================

#include "overlay_core.h"
#include "hooks/dxgi_hook.h"
#include "ipc/shared_memory.h"
#include "ipc/pipe_client.h"
#include "input/input_hook.h"

#include <MinHook.h>
#include <string>

namespace Overlay
{

// ── helpers ────────────────────────────────────────────────
static std::wstring BuildIpcName()
{
    // The launcher writes its own PID into an environment variable
    // before injecting so the DLL can create uniquely-named IPC
    // objects.  Fall back to a fixed name when unset.
    wchar_t buf[64]{};
    DWORD len = GetEnvironmentVariableW(L"OVERLAY_LAUNCHER_PID", buf, 64);
    if (len == 0 || len >= 64)
        return L"007Overlay_Default";
    return std::wstring(L"007Overlay_") + buf;
}

// ── Lifecycle ──────────────────────────────────────────────

void Initialise()
{
    if (g_initialised.exchange(true))
        return; // already initialised

    g_ipcName = BuildIpcName();

    // 1. Initialise the MinHook library.
    if (MH_Initialize() != MH_OK)
    {
        OutputDebugStringW(L"[007Overlay] MH_Initialize failed\n");
        g_initialised = false;
        return;
    }

    // 2. Install the DXGI Present hook.
    //    This blocks until the game creates its swap chain, polling
    //    with a short sleep so we don't spin-burn the CPU.
    if (!DxgiHook::Install())
    {
        OutputDebugStringW(L"[007Overlay] DXGI hook installation failed\n");
        MH_Uninitialize();
        g_initialised = false;
        return;
    }

    // 3. Open the shared-memory region written by the launcher.
    if (!SharedMemory::Open(g_ipcName))
    {
        OutputDebugStringW(L"[007Overlay] SharedMemory::Open failed\n");
    }

    // 4. Connect the named-pipe command channel.
    if (!PipeClient::Connect(g_ipcName))
    {
        OutputDebugStringW(L"[007Overlay] PipeClient::Connect failed\n");
    }

    // 5. Install the input (WndProc) hook.
    InputHook::Install();

    OutputDebugStringW(L"[007Overlay] Initialisation complete\n");
}

void Shutdown()
{
    if (!g_initialised)
        return;

    g_shutdown = true;
    g_active   = false;

    InputHook::Remove();
    DxgiHook::Remove();
    SharedMemory::Close();
    PipeClient::Disconnect();

    MH_Uninitialize();

    g_initialised = false;
    OutputDebugStringW(L"[007Overlay] Shutdown complete\n");
}

// ── State ──────────────────────────────────────────────────

bool IsActive()
{
    return g_active.load();
}

void Toggle()
{
    g_active = !g_active;
    // Notify the launcher so it can start/stop rendering the
    // overlay WebView.
    PipeClient::SendCommand(g_active ? "OVERLAY_SHOW" : "OVERLAY_HIDE");
    OutputDebugStringW(g_active ? L"[007Overlay] Overlay shown\n"
                                : L"[007Overlay] Overlay hidden\n");
}

} // namespace Overlay
