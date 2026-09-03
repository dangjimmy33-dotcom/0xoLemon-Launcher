#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <dinput.h>
#include <stdio.h>
#include <stdbool.h>

static HMODULE g_realDinput8 = NULL;
static FARPROC g_pDirectInput8Create = NULL;
static FARPROC g_pDllCanUnloadNow = NULL;
static FARPROC g_pDllGetClassObject = NULL;
static FARPROC g_pDllRegisterServer = NULL;
static FARPROC g_pDllUnregisterServer = NULL;
static FARPROC g_pGetdfDIJoystick = NULL;

static volatile bool g_overlayOpen = false;
static bool g_enableLog = false;

// Original function pointers
static BOOL (WINAPI *g_origClipCursor)(const RECT* lpRect) = ClipCursor;
static BOOL (WINAPI *g_origSetCursorPos)(int X, int Y) = SetCursorPos;
static HWND (WINAPI *g_origSetCapture)(HWND hWnd) = SetCapture;
static int  (WINAPI *g_origShowCursor)(BOOL bShow) = ShowCursor;

static void LoadConfig() {
    char iniPath[MAX_PATH];
    GetCurrentDirectoryA(MAX_PATH, iniPath);
    strcat_s(iniPath, "\\dinput8.ini");
    g_enableLog = (GetPrivateProfileIntA("Settings", "enable_log", 0, iniPath) != 0);
}

static void LogMsg(const char* format, ...) {
    if (!g_enableLog) return;
    FILE* f = fopen("gse_dinput8_bridge.log", "a");
    if (f) {
        SYSTEMTIME st;
        GetLocalTime(&st);
        fprintf(f, "[%02d:%02d:%02d.%03d] ", st.wHour, st.wMinute, st.wSecond, st.wMilliseconds);
        va_list args;
        va_start(args, format);
        vfprintf(f, format, args);
        va_end(args);
        fclose(f);
    }
}

// Hooked Win32 functions to release mouse when overlay is active
static BOOL WINAPI Hooked_ClipCursor(const RECT* lpRect) {
    if (g_overlayOpen) {
        return g_origClipCursor ? g_origClipCursor(NULL) : TRUE;
    }
    return g_origClipCursor ? g_origClipCursor(lpRect) : ClipCursor(lpRect);
}

static BOOL WINAPI Hooked_SetCursorPos(int X, int Y) {
    if (g_overlayOpen) {
        // Prevent game from locking mouse to screen center while overlay is open
        return TRUE;
    }
    return g_origSetCursorPos ? g_origSetCursorPos(X, Y) : SetCursorPos(X, Y);
}

static HWND WINAPI Hooked_SetCapture(HWND hWnd) {
    if (g_overlayOpen) {
        ReleaseCapture();
        return NULL;
    }
    return g_origSetCapture ? g_origSetCapture(hWnd) : SetCapture(hWnd);
}

static int WINAPI Hooked_ShowCursor(BOOL bShow) {
    if (g_overlayOpen) {
        if (g_origShowCursor) return g_origShowCursor(TRUE);
        return ShowCursor(TRUE);
    }
    return g_origShowCursor ? g_origShowCursor(bShow) : ShowCursor(bShow);
}

static void HookIAT(HMODULE hMod, const char* szDllName, const char* szFuncName, PVOID pNewFunc, PVOID* ppOldFunc) {
    if (!hMod) hMod = GetModuleHandle(NULL);
    PIMAGE_DOS_HEADER pDosHeader = (PIMAGE_DOS_HEADER)hMod;
    if (pDosHeader->e_magic != IMAGE_DOS_SIGNATURE) return;
    PIMAGE_NT_HEADERS pNtHeaders = (PIMAGE_NT_HEADERS)((BYTE*)hMod + pDosHeader->e_lfanew);
    if (pNtHeaders->Signature != IMAGE_NT_SIGNATURE) return;

    PIMAGE_IMPORT_DESCRIPTOR pImportDesc = (PIMAGE_IMPORT_DESCRIPTOR)((BYTE*)hMod + 
        pNtHeaders->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT].VirtualAddress);

    for (; pImportDesc->Name; pImportDesc++) {
        const char* pszModName = (const char*)((BYTE*)hMod + pImportDesc->Name);
        if (_stricmp(pszModName, szDllName) == 0) {
            PIMAGE_THUNK_DATA pThunk = (PIMAGE_THUNK_DATA)((BYTE*)hMod + pImportDesc->FirstThunk);
            PIMAGE_THUNK_DATA pOriginalThunk = (PIMAGE_THUNK_DATA)((BYTE*)hMod + pImportDesc->OriginalFirstThunk);
            if (!pOriginalThunk) pOriginalThunk = pThunk;

            for (; pOriginalThunk->u1.Function; pOriginalThunk++, pThunk++) {
                if (IMAGE_SNAP_BY_ORDINAL(pOriginalThunk->u1.Ordinal)) continue;
                PIMAGE_IMPORT_BY_NAME pImportByName = (PIMAGE_IMPORT_BY_NAME)((BYTE*)hMod + pOriginalThunk->u1.AddressOfData);
                if (strcmp((char*)pImportByName->Name, szFuncName) == 0) {
                    DWORD oldProtect;
                    VirtualProtect(&pThunk->u1.Function, sizeof(PVOID), PAGE_EXECUTE_READWRITE, &oldProtect);
                    if (ppOldFunc && *ppOldFunc == NULL) {
                        *ppOldFunc = (PVOID)pThunk->u1.Function;
                    }
                    pThunk->u1.Function = (ULONG_PTR)pNewFunc;
                    VirtualProtect(&pThunk->u1.Function, sizeof(PVOID), oldProtect, &oldProtect);
                    LogMsg("[IAT-Hook] Successfully hooked %s!%s in %p\n", szDllName, szFuncName, hMod);
                    return;
                }
            }
        }
    }
}

static void InstallCursorHooks() {
    HMODULE hExe = GetModuleHandle(NULL);
    HookIAT(hExe, "USER32.dll", "ClipCursor", (PVOID)Hooked_ClipCursor, (PVOID*)&g_origClipCursor);
    HookIAT(hExe, "USER32.dll", "SetCursorPos", (PVOID)Hooked_SetCursorPos, (PVOID*)&g_origSetCursorPos);
    HookIAT(hExe, "USER32.dll", "SetCapture", (PVOID)Hooked_SetCapture, (PVOID*)&g_origSetCapture);
    HookIAT(hExe, "USER32.dll", "ShowCursor", (PVOID)Hooked_ShowCursor, (PVOID*)&g_origShowCursor);
}

static void LoadRealDinput8() {
    if (g_realDinput8) return;
    wchar_t sysPath[MAX_PATH];
    GetSystemDirectoryW(sysPath, MAX_PATH);
    wcscat_s(sysPath, L"\\dinput8.dll");
    g_realDinput8 = LoadLibraryW(sysPath);
    if (g_realDinput8) {
        g_pDirectInput8Create = GetProcAddress(g_realDinput8, "DirectInput8Create");
        g_pDllCanUnloadNow = GetProcAddress(g_realDinput8, "DllCanUnloadNow");
        g_pDllGetClassObject = GetProcAddress(g_realDinput8, "DllGetClassObject");
        g_pDllRegisterServer = GetProcAddress(g_realDinput8, "DllRegisterServer");
        g_pDllUnregisterServer = GetProcAddress(g_realDinput8, "DllUnregisterServer");
        g_pGetdfDIJoystick = GetProcAddress(g_realDinput8, "GetdfDIJoystick");
        LogMsg("Loaded real dinput8.dll from: %ls\n", sysPath);
    } else {
        LogMsg("ERROR: Failed to load real dinput8.dll from System32!\n");
    }
}

extern "C" {
    HRESULT WINAPI Proxy_DirectInput8Create(HINSTANCE hinst, DWORD dwVersion, REFIID riidltf, LPVOID *ppvOut, LPUNKNOWN punkOuter) {
        LoadRealDinput8();
        if (g_pDirectInput8Create) {
            typedef HRESULT(WINAPI *pfn_t)(HINSTANCE, DWORD, REFIID, LPVOID*, LPUNKNOWN);
            return ((pfn_t)g_pDirectInput8Create)(hinst, dwVersion, riidltf, ppvOut, punkOuter);
        }
        return E_FAIL;
    }

    HRESULT WINAPI Proxy_DllCanUnloadNow() {
        LoadRealDinput8();
        if (g_pDllCanUnloadNow) {
            typedef HRESULT(WINAPI *pfn_t)();
            return ((pfn_t)g_pDllCanUnloadNow)();
        }
        return S_FALSE;
    }

    HRESULT WINAPI Proxy_DllGetClassObject(REFCLSID rclsid, REFIID riid, LPVOID *ppv) {
        LoadRealDinput8();
        if (g_pDllGetClassObject) {
            typedef HRESULT(WINAPI *pfn_t)(REFCLSID, REFIID, LPVOID*);
            return ((pfn_t)g_pDllGetClassObject)(rclsid, riid, ppv);
        }
        return CLASS_E_CLASSNOTAVAILABLE;
    }

    HRESULT WINAPI Proxy_DllRegisterServer() {
        LoadRealDinput8();
        if (g_pDllRegisterServer) {
            typedef HRESULT(WINAPI *pfn_t)();
            return ((pfn_t)g_pDllRegisterServer)();
        }
        return E_FAIL;
    }

    HRESULT WINAPI Proxy_DllUnregisterServer() {
        LoadRealDinput8();
        if (g_pDllUnregisterServer) {
            typedef HRESULT(WINAPI *pfn_t)();
            return ((pfn_t)g_pDllUnregisterServer)();
        }
        return E_FAIL;
    }

    LPCDIDATAFORMAT WINAPI Proxy_GetdfDIJoystick() {
        LoadRealDinput8();
        if (g_pGetdfDIJoystick) {
            typedef LPCDIDATAFORMAT(WINAPI *pfn_t)();
            return ((pfn_t)g_pGetdfDIJoystick)();
        }
        return NULL;
    }
}

typedef void* (*SteamFriends_Fn)();
typedef void (*ActivateGameOverlay_Fn)(void* self, const char* pchDialog);

static void ToggleGSEOverlay() {
    g_overlayOpen = !g_overlayOpen;
    LogMsg("--> Overlay state toggled: %s\n", g_overlayOpen ? "OPEN" : "CLOSED");

    if (g_overlayOpen) {
        ClipCursor(NULL);
        ReleaseCapture();
        SetCursor(LoadCursor(NULL, IDC_ARROW));
        while (ShowCursor(TRUE) < 0);
    }

    HMODULE hSteamApi = GetModuleHandleA("steam_api64.dll");
    if (!hSteamApi) {
        hSteamApi = LoadLibraryA("steam_api64.dll");
    }

    if (hSteamApi) {
        SteamFriends_Fn pSteamFriends = (SteamFriends_Fn)GetProcAddress(hSteamApi, "SteamFriends");
        ActivateGameOverlay_Fn pActivateOverlay = (ActivateGameOverlay_Fn)GetProcAddress(hSteamApi, "SteamAPI_ISteamFriends_ActivateGameOverlay");

        if (pSteamFriends && pActivateOverlay) {
            void* pFriends = pSteamFriends();
            if (pFriends) {
                pActivateOverlay(pFriends, "Community");
            }
        }
    }
}

static DWORD WINAPI InputWatcherThread(LPVOID lpParam) {
    LoadConfig();
    LogMsg("GSE Input Watcher Thread started successfully (enable_log=%d).\n", g_enableLog ? 1 : 0);
    Sleep(2500); // Allow game & steam_api64 to initialize

    InstallCursorHooks();

    bool wasPressed = false;
    bool wasEscPressed = false;

    while (true) {
        Sleep(15);

        // Keep mouse free while overlay is open
        if (g_overlayOpen) {
            ClipCursor(NULL);
            ReleaseCapture();
            SetCursor(LoadCursor(NULL, IDC_ARROW));
        }

        bool shiftHeld = (GetAsyncKeyState(VK_SHIFT) & 0x8000) != 0;
        bool tabHeld   = (GetAsyncKeyState(VK_TAB) & 0x8000) != 0;
        bool f8Pressed = (GetAsyncKeyState(VK_F8) & 0x8000) != 0;
        bool insPressed= (GetAsyncKeyState(VK_INSERT) & 0x8000) != 0;
        bool homePressed=(GetAsyncKeyState(VK_HOME) & 0x8000) != 0;
        bool escPressed =(GetAsyncKeyState(VK_ESCAPE) & 0x8000) != 0;

        bool isTriggered = (shiftHeld && tabHeld) || f8Pressed || insPressed || homePressed;

        if (isTriggered && !wasPressed) {
            wasPressed = true;
            LogMsg("Hotkey detected: [Shift+Tab=%d, F8=%d, Insert=%d, Home=%d]\n",
                (shiftHeld && tabHeld), f8Pressed, insPressed, homePressed);
            ToggleGSEOverlay();
        } else if (!isTriggered && wasPressed) {
            wasPressed = false;
        }

        // Close overlay if ESC is pressed while open
        if (g_overlayOpen && escPressed && !wasEscPressed) {
            wasEscPressed = true;
            LogMsg("ESC pressed: closing overlay.\n");
            ToggleGSEOverlay();
        } else if (!escPressed && wasEscPressed) {
            wasEscPressed = false;
        }
    }
    return 0;
}

BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpvReserved) {
    switch (fdwReason) {
    case DLL_PROCESS_ATTACH:
        DisableThreadLibraryCalls(hinstDLL);
        LoadConfig();
        LogMsg("=====================================================\n");
        LogMsg("GSE DirectInput8 Bridge DLL attached to process.\n");
        LogMsg("=====================================================\n");
        LoadRealDinput8();
        CreateThread(NULL, 0, InputWatcherThread, NULL, 0, NULL);
        break;
    case DLL_PROCESS_DETACH:
        if (g_realDinput8) {
            FreeLibrary(g_realDinput8);
            g_realDinput8 = NULL;
        }
        break;
    }
    return TRUE;
}