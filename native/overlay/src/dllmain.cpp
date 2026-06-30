// ============================================================
// 007Launcher In-Game Overlay — DLL Entry Point
// ============================================================
// This DLL is injected into a game process by the Launcher's
// Injector module. On attach it initialises the overlay system;
// on detach it cleans everything up.
// ============================================================

#define WIN32_LEAN_AND_MEAN
#include <Windows.h>
#include "overlay_core.h"

HMODULE g_hModule = nullptr;

BOOL APIENTRY DllMain(HMODULE hModule, DWORD reason, LPVOID /*reserved*/)
{
    switch (reason)
    {
    case DLL_PROCESS_ATTACH:
        g_hModule = hModule;
        DisableThreadLibraryCalls(hModule);
        // Initialise on a worker thread so DllMain returns quickly
        // and does not block the loader lock.
        CreateThread(nullptr, 0,
            [](LPVOID) -> DWORD {
                Overlay::Initialise();
                return 0;
            },
            nullptr, 0, nullptr);
        break;

    case DLL_PROCESS_DETACH:
        Overlay::Shutdown();
        break;
    }

    return TRUE;
}
