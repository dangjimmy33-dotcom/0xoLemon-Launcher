// dwmapi.dll proxy - forwards all DWM API calls to system DLL, then loads OxoSteamCore.dll
// This DLL is placed in Steam's root directory and gets loaded before any game code

#include <windows.h>
#include <cstdio>

// Load the real system dwmapi.dll and forward all exports
// We need to forward to the REAL system DLL, not use pragma export
// because Steam loads us BEFORE the system DLL

static HMODULE g_systemDwmapi = nullptr;
static HMODULE g_coreHook = nullptr;

// Forward declarations for all DWM functions
typedef HRESULT (WINAPI *DwmDefWindowProc_t)(HWND, UINT, WPARAM, LPARAM, LRESULT*);
typedef HRESULT (WINAPI *DwmEnableBlurBehindWindow_t)(HWND, const void*);
typedef HRESULT (WINAPI *DwmEnableComposition_t)(UINT);
typedef HRESULT (WINAPI *DwmExtendFrameIntoClientArea_t)(HWND, const void*);
typedef HRESULT (WINAPI *DwmFlush_t)();
typedef HRESULT (WINAPI *DwmGetColorizationColor_t)(DWORD*, BOOL*);
typedef HRESULT (WINAPI *DwmGetCompositionTimingInfo_t)(HWND, void*);
typedef HRESULT (WINAPI *DwmGetWindowAttribute_t)(HWND, DWORD, PVOID, DWORD);
typedef HRESULT (WINAPI *DwmIsCompositionEnabled_t)(BOOL*);
typedef HRESULT (WINAPI *DwmRegisterThumbnail_t)(HWND, HWND, void*);
typedef HRESULT (WINAPI *DwmSetWindowAttribute_t)(HWND, DWORD, LPCVOID, DWORD);
typedef HRESULT (WINAPI *DwmUnregisterThumbnail_t)(void*);
typedef HRESULT (WINAPI *DwmUpdateThumbnailProperties_t)(void*, const void*);

// Export wrappers - just pass through to system DLL
extern "C" {
    __declspec(dllexport) HRESULT WINAPI DwmDefWindowProc(HWND hWnd, UINT msg, WPARAM wParam, LPARAM lParam, LRESULT* plResult) {
        auto fn = (DwmDefWindowProc_t)GetProcAddress(g_systemDwmapi, "DwmDefWindowProc");
        return fn ? fn(hWnd, msg, wParam, lParam, plResult) : E_FAIL;
    }
    
    __declspec(dllexport) HRESULT WINAPI DwmEnableBlurBehindWindow(HWND hWnd, const void* pBlurBehind) {
        auto fn = (DwmEnableBlurBehindWindow_t)GetProcAddress(g_systemDwmapi, "DwmEnableBlurBehindWindow");
        return fn ? fn(hWnd, pBlurBehind) : E_FAIL;
    }
    
    __declspec(dllexport) HRESULT WINAPI DwmEnableComposition(UINT uCompositionAction) {
        auto fn = (DwmEnableComposition_t)GetProcAddress(g_systemDwmapi, "DwmEnableComposition");
        return fn ? fn(uCompositionAction) : E_FAIL;
    }
    
    __declspec(dllexport) HRESULT WINAPI DwmExtendFrameIntoClientArea(HWND hWnd, const void* pMarInset) {
        auto fn = (DwmExtendFrameIntoClientArea_t)GetProcAddress(g_systemDwmapi, "DwmExtendFrameIntoClientArea");
        return fn ? fn(hWnd, pMarInset) : E_FAIL;
    }
    
    __declspec(dllexport) HRESULT WINAPI DwmFlush() {
        auto fn = (DwmFlush_t)GetProcAddress(g_systemDwmapi, "DwmFlush");
        return fn ? fn() : E_FAIL;
    }
    
    __declspec(dllexport) HRESULT WINAPI DwmGetColorizationColor(DWORD* pcrColorization, BOOL* pfOpaqueBlend) {
        auto fn = (DwmGetColorizationColor_t)GetProcAddress(g_systemDwmapi, "DwmGetColorizationColor");
        return fn ? fn(pcrColorization, pfOpaqueBlend) : E_FAIL;
    }
    
    __declspec(dllexport) HRESULT WINAPI DwmGetCompositionTimingInfo(HWND hwnd, void* pTimingInfo) {
        auto fn = (DwmGetCompositionTimingInfo_t)GetProcAddress(g_systemDwmapi, "DwmGetCompositionTimingInfo");
        return fn ? fn(hwnd, pTimingInfo) : E_FAIL;
    }
    
    __declspec(dllexport) HRESULT WINAPI DwmGetWindowAttribute(HWND hwnd, DWORD dwAttribute, PVOID pvAttribute, DWORD cbAttribute) {
        auto fn = (DwmGetWindowAttribute_t)GetProcAddress(g_systemDwmapi, "DwmGetWindowAttribute");
        return fn ? fn(hwnd, dwAttribute, pvAttribute, cbAttribute) : E_FAIL;
    }
    
    __declspec(dllexport) HRESULT WINAPI DwmIsCompositionEnabled(BOOL* pfEnabled) {
        auto fn = (DwmIsCompositionEnabled_t)GetProcAddress(g_systemDwmapi, "DwmIsCompositionEnabled");
        return fn ? fn(pfEnabled) : E_FAIL;
    }
    
    __declspec(dllexport) HRESULT WINAPI DwmRegisterThumbnail(HWND hwndDestination, HWND hwndSource, void* pThumbnailId) {
        auto fn = (DwmRegisterThumbnail_t)GetProcAddress(g_systemDwmapi, "DwmRegisterThumbnail");
        return fn ? fn(hwndDestination, hwndSource, pThumbnailId) : E_FAIL;
    }
    
    __declspec(dllexport) HRESULT WINAPI DwmSetWindowAttribute(HWND hwnd, DWORD dwAttribute, LPCVOID pvAttribute, DWORD cbAttribute) {
        auto fn = (DwmSetWindowAttribute_t)GetProcAddress(g_systemDwmapi, "DwmSetWindowAttribute");
        return fn ? fn(hwnd, dwAttribute, pvAttribute, cbAttribute) : E_FAIL;
    }
    
    __declspec(dllexport) HRESULT WINAPI DwmUnregisterThumbnail(void* hThumbnailId) {
        auto fn = (DwmUnregisterThumbnail_t)GetProcAddress(g_systemDwmapi, "DwmUnregisterThumbnail");
        return fn ? fn(hThumbnailId) : E_FAIL;
    }
    
    __declspec(dllexport) HRESULT WINAPI DwmUpdateThumbnailProperties(void* hThumbnailId, const void* ptnProperties) {
        auto fn = (DwmUpdateThumbnailProperties_t)GetProcAddress(g_systemDwmapi, "DwmUpdateThumbnailProperties");
        return fn ? fn(hThumbnailId, ptnProperties) : E_FAIL;
    }
}

BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpvReserved) {
    if (fdwReason == DLL_PROCESS_ATTACH) {
        DisableThreadLibraryCalls(hinstDLL);

        // Load system dwmapi.dll FIRST
        char systemPath[MAX_PATH];
        GetSystemDirectoryA(systemPath, MAX_PATH);
        strcat_s(systemPath, "\\dwmapi.dll");
        g_systemDwmapi = LoadLibraryA(systemPath);
        
        if (!g_systemDwmapi) {
            OutputDebugStringA("[OxoProxy] ERROR: Failed to load system dwmapi.dll\n");
            return FALSE;
        }
        
        OutputDebugStringA("[OxoProxy] System dwmapi.dll loaded\n");

        // Get our own path to find OxoSteamCore.dll
        char selfPath[MAX_PATH];
        GetModuleFileNameA(hinstDLL, selfPath, MAX_PATH);
        
        // Extract directory
        char* lastSlash = strrchr(selfPath, '\\');
        if (lastSlash) *lastSlash = '\0';

        // Load our hook DLL
        char corePath[MAX_PATH];
        sprintf_s(corePath, "%s\\OxoSteamCore.dll", selfPath);
        g_coreHook = LoadLibraryA(corePath);

        if (g_coreHook) {
            OutputDebugStringA("[OxoProxy] OxoSteamCore.dll loaded\n");
            // Call initialization function if exists
            typedef void (*InitFunc)();
            InitFunc init = (InitFunc)GetProcAddress(g_coreHook, "OxoInitialize");
            if (init) {
                OutputDebugStringA("[OxoProxy] Calling OxoInitialize()...\n");
                init();
            } else {
                OutputDebugStringA("[OxoProxy] WARNING: OxoInitialize not found\n");
            }
        } else {
            char msg[512];
            sprintf_s(msg, "[OxoProxy] ERROR: Failed to load OxoSteamCore.dll from %s (error %d)\n", 
                     corePath, GetLastError());
            OutputDebugStringA(msg);
        }
    } else if (fdwReason == DLL_PROCESS_DETACH) {
        if (g_coreHook) {
            typedef void (*ShutdownFunc)();
            ShutdownFunc shutdown = (ShutdownFunc)GetProcAddress(g_coreHook, "OxoShutdown");
            if (shutdown) shutdown();
            FreeLibrary(g_coreHook);
        }
        if (g_systemDwmapi) {
            FreeLibrary(g_systemDwmapi);
        }
    }
    return TRUE;
}
