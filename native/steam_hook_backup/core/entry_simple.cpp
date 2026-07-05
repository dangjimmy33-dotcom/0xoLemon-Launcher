#include <Windows.h>
#include <cstdio>

// Simplified version without Detours - just for testing DLL injection

extern "C" __declspec(dllexport) void OxoInitialize() {
    OutputDebugStringA("[OxoHook] ========================================\n");
    OutputDebugStringA("[OxoHook] OxoSteamCore loaded successfully!\n");
    OutputDebugStringA("[OxoHook] DLL injection working!\n");
    OutputDebugStringA("[OxoHook] ========================================\n");
    
    // Get our path
    char dllPath[MAX_PATH];
    HMODULE hSelf = nullptr;
    GetModuleHandleExA(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        (LPCSTR)OxoInitialize,
        &hSelf
    );
    
    if (hSelf && GetModuleFileNameA(hSelf, dllPath, MAX_PATH)) {
        char msg[512];
        sprintf_s(msg, "[OxoHook] DLL path: %s\n", dllPath);
        OutputDebugStringA(msg);
    }
    
    // Check if Steam
    if (GetModuleHandleA("steamclient64.dll")) {
        OutputDebugStringA("[OxoHook] steamclient64.dll detected - running inside Steam!\n");
    } else {
        OutputDebugStringA("[OxoHook] steamclient64.dll NOT found\n");
    }
}

extern "C" __declspec(dllexport) void OxoShutdown() {
    OutputDebugStringA("[OxoHook] Shutting down...\n");
}

BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpvReserved) {
    if (fdwReason == DLL_PROCESS_ATTACH) {
        DisableThreadLibraryCalls(hinstDLL);
    } else if (fdwReason == DLL_PROCESS_DETACH) {
        OxoShutdown();
    }
    return TRUE;
}
