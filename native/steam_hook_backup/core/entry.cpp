#include "lua_loader.h"
#include "hooks/package_hooks.h"
#include <Windows.h>
#include <detours.h>

static bool g_initialized = false;
static bool g_hooksInstalled = false;
static char g_steamPath[MAX_PATH] = {0};

// Original LoadLibraryA/W pointers
static decltype(&LoadLibraryA) Original_LoadLibraryA = LoadLibraryA;
static decltype(&LoadLibraryW) Original_LoadLibraryW = LoadLibraryW;
static decltype(&LoadLibraryExA) Original_LoadLibraryExA = LoadLibraryExA;
static decltype(&LoadLibraryExW) Original_LoadLibraryExW = LoadLibraryExW;

// Find Steam installation path
static bool FindSteamPath(char* outPath, size_t maxLen) {
    HMODULE hSelf = nullptr;
    GetModuleHandleExA(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        (LPCSTR)FindSteamPath,
        &hSelf
    );
    
    if (!hSelf) return false;
    
    char dllPath[MAX_PATH];
    if (!GetModuleFileNameA(hSelf, dllPath, MAX_PATH)) return false;
    
    char* lastSlash = strrchr(dllPath, '\\');
    if (!lastSlash) return false;
    
    *lastSlash = '\0';
    strncpy_s(outPath, maxLen, dllPath, _TRUNCATE);
    return true;
}

// Install Steam hooks once steamclient64.dll is detected
static void InstallSteamHooks() {
    if (g_hooksInstalled) return;
    
    OutputDebugStringA("[OxoHook] ========================================\n");
    OutputDebugStringA("[OxoHook] steamclient64.dll loaded - installing hooks\n");
    OutputDebugStringA("[OxoHook] ========================================\n");
    
    // Small delay to let Steam initialize
    Sleep(100);
    
    // Initialize lua loader
    OutputDebugStringA("[OxoHook] Starting Lua file watcher...\n");
    OxoLua::Initialize(g_steamPath);
    
    // Install ownership hooks
    OutputDebugStringA("[OxoHook] Installing ownership hooks...\n");
    OxoHooks::InstallPackageHooks();
    
    g_hooksInstalled = true;
}

// Hooked LoadLibraryA - detect steamclient64.dll loading
static HMODULE WINAPI Hook_LoadLibraryA(LPCSTR lpLibFileName) {
    HMODULE result = Original_LoadLibraryA(lpLibFileName);
    
    if (result && lpLibFileName) {
        // Check if this is steamclient64.dll
        const char* filename = strrchr(lpLibFileName, '\\');
        if (!filename) filename = lpLibFileName;
        else filename++; // Skip the backslash
        
        if (_stricmp(filename, "steamclient64.dll") == 0) {
            char msg[512];
            sprintf_s(msg, "[OxoHook] Detected LoadLibraryA(\"%s\") = 0x%p\n", 
                     lpLibFileName, (void*)result);
            OutputDebugStringA(msg);
            
            InstallSteamHooks();
        }
    }
    
    return result;
}

// Hooked LoadLibraryW - detect steamclient64.dll loading
static HMODULE WINAPI Hook_LoadLibraryW(LPCWSTR lpLibFileName) {
    HMODULE result = Original_LoadLibraryW(lpLibFileName);
    
    if (result && lpLibFileName) {
        // Convert to narrow string for comparison
        char narrowName[MAX_PATH];
        WideCharToMultiByte(CP_ACP, 0, lpLibFileName, -1, narrowName, MAX_PATH, nullptr, nullptr);
        
        const char* filename = strrchr(narrowName, '\\');
        if (!filename) filename = narrowName;
        else filename++;
        
        if (_stricmp(filename, "steamclient64.dll") == 0) {
            char msg[512];
            sprintf_s(msg, "[OxoHook] Detected LoadLibraryW(\"%s\") = 0x%p\n", 
                     narrowName, (void*)result);
            OutputDebugStringA(msg);
            
            InstallSteamHooks();
        }
    }
    
    return result;
}

// Hooked LoadLibraryExA
static HMODULE WINAPI Hook_LoadLibraryExA(LPCSTR lpLibFileName, HANDLE hFile, DWORD dwFlags) {
    HMODULE result = Original_LoadLibraryExA(lpLibFileName, hFile, dwFlags);
    
    if (result && lpLibFileName) {
        const char* filename = strrchr(lpLibFileName, '\\');
        if (!filename) filename = lpLibFileName;
        else filename++;
        
        if (_stricmp(filename, "steamclient64.dll") == 0) {
            char msg[512];
            sprintf_s(msg, "[OxoHook] Detected LoadLibraryExA(\"%s\") = 0x%p\n", 
                     lpLibFileName, (void*)result);
            OutputDebugStringA(msg);
            
            InstallSteamHooks();
        }
    }
    
    return result;
}

// Hooked LoadLibraryExW
static HMODULE WINAPI Hook_LoadLibraryExW(LPCWSTR lpLibFileName, HANDLE hFile, DWORD dwFlags) {
    HMODULE result = Original_LoadLibraryExW(lpLibFileName, hFile, dwFlags);
    
    if (result && lpLibFileName) {
        char narrowName[MAX_PATH];
        WideCharToMultiByte(CP_ACP, 0, lpLibFileName, -1, narrowName, MAX_PATH, nullptr, nullptr);
        
        const char* filename = strrchr(narrowName, '\\');
        if (!filename) filename = narrowName;
        else filename++;
        
        if (_stricmp(filename, "steamclient64.dll") == 0) {
            char msg[512];
            sprintf_s(msg, "[OxoHook] Detected LoadLibraryExW(\"%s\") = 0x%p\n", 
                     narrowName, (void*)result);
            OutputDebugStringA(msg);
            
            InstallSteamHooks();
        }
    }
    
    return result;
}

// Exported initialization function called by proxy DLL
extern "C" __declspec(dllexport) void OxoInitialize() {
    if (g_initialized) return;
    
    OutputDebugStringA("[OxoHook] ========================================\n");
    OutputDebugStringA("[OxoHook] OxoSteamCore initializing...\n");
    OutputDebugStringA("[OxoHook] ========================================\n");
    
    // Find Steam path
    if (!FindSteamPath(g_steamPath, MAX_PATH)) {
        OutputDebugStringA("[OxoHook] ERROR: Failed to find Steam path\n");
        return;
    }
    
    char msg[512];
    sprintf_s(msg, "[OxoHook] Steam path: %s\n", g_steamPath);
    OutputDebugStringA(msg);
    
    // Check if steamclient64.dll is already loaded
    HMODULE hSteamClient = GetModuleHandleA("steamclient64.dll");
    if (hSteamClient) {
        OutputDebugStringA("[OxoHook] steamclient64.dll already loaded!\n");
        InstallSteamHooks();
    } else {
        OutputDebugStringA("[OxoHook] steamclient64.dll not loaded yet\n");
        OutputDebugStringA("[OxoHook] Hooking LoadLibrary to detect when it loads...\n");
        
        // Install LoadLibrary hooks to intercept steamclient64.dll loading
        DetourTransactionBegin();
        DetourUpdateThread(GetCurrentThread());
        
        DetourAttach(&(PVOID&)Original_LoadLibraryA, Hook_LoadLibraryA);
        DetourAttach(&(PVOID&)Original_LoadLibraryW, Hook_LoadLibraryW);
        DetourAttach(&(PVOID&)Original_LoadLibraryExA, Hook_LoadLibraryExA);
        DetourAttach(&(PVOID&)Original_LoadLibraryExW, Hook_LoadLibraryExW);
        
        LONG result = DetourTransactionCommit();
        
        if (result == NO_ERROR) {
            OutputDebugStringA("[OxoHook] LoadLibrary hooks installed - waiting for steamclient64.dll\n");
        } else {
            sprintf_s(msg, "[OxoHook] ERROR: Failed to hook LoadLibrary (error %ld)\n", result);
            OutputDebugStringA(msg);
        }
    }
    
    g_initialized = true;
    OutputDebugStringA("[OxoHook] ========================================\n");
    OutputDebugStringA("[OxoHook] Ready!\n");
    OutputDebugStringA("[OxoHook] ========================================\n");
}

// Exported shutdown function
extern "C" __declspec(dllexport) void OxoShutdown() {
    if (!g_initialized) return;
    
    OutputDebugStringA("[OxoHook] Shutting down...\n");
    
    // Unhook LoadLibrary
    if (!g_hooksInstalled) {
        DetourTransactionBegin();
        DetourUpdateThread(GetCurrentThread());
        DetourDetach(&(PVOID&)Original_LoadLibraryA, Hook_LoadLibraryA);
        DetourDetach(&(PVOID&)Original_LoadLibraryW, Hook_LoadLibraryW);
        DetourDetach(&(PVOID&)Original_LoadLibraryExA, Hook_LoadLibraryExA);
        DetourDetach(&(PVOID&)Original_LoadLibraryExW, Hook_LoadLibraryExW);
        DetourTransactionCommit();
    }
    
    if (g_hooksInstalled) {
        OxoHooks::UninstallPackageHooks();
        OxoLua::Shutdown();
    }
    
    g_initialized = false;
    g_hooksInstalled = false;
}

BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpvReserved) {
    if (fdwReason == DLL_PROCESS_ATTACH) {
        DisableThreadLibraryCalls(hinstDLL);
    } else if (fdwReason == DLL_PROCESS_DETACH) {
        OxoShutdown();
    }
    return TRUE;
}
