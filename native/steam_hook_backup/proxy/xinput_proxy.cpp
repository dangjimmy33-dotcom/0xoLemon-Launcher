// xinput1_4.dll proxy - backup injection point
#include <windows.h>
#include <cstdio>

#pragma comment(linker, "/EXPORT:XInputGetState=XINPUT_SYSTEM.XInputGetState,@2")
#pragma comment(linker, "/EXPORT:XInputSetState=XINPUT_SYSTEM.XInputSetState,@3")
#pragma comment(linker, "/EXPORT:XInputGetCapabilities=XINPUT_SYSTEM.XInputGetCapabilities,@4")
#pragma comment(linker, "/EXPORT:XInputEnable=XINPUT_SYSTEM.XInputEnable,@5")
#pragma comment(linker, "/EXPORT:XInputGetDSoundAudioDeviceGuids=XINPUT_SYSTEM.XInputGetDSoundAudioDeviceGuids,@6")
#pragma comment(linker, "/EXPORT:XInputGetBatteryInformation=XINPUT_SYSTEM.XInputGetBatteryInformation,@7")
#pragma comment(linker, "/EXPORT:XInputGetKeystroke=XINPUT_SYSTEM.XInputGetKeystroke,@8")

static HMODULE g_systemXInput = nullptr;
static HMODULE g_coreHook = nullptr;

BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpvReserved) {
    if (fdwReason == DLL_PROCESS_ATTACH) {
        DisableThreadLibraryCalls(hinstDLL);

        char selfPath[MAX_PATH];
        GetModuleFileNameA(hinstDLL, selfPath, MAX_PATH);
        char* lastSlash = strrchr(selfPath, '\\');
        if (lastSlash) *lastSlash = '\0';

        // Load system xinput1_4.dll
        char systemPath[MAX_PATH];
        GetSystemDirectoryA(systemPath, MAX_PATH);
        strcat_s(systemPath, "\\xinput1_4.dll");
        g_systemXInput = LoadLibraryA(systemPath);

        // Load hook core if dwmapi proxy hasn't already
        char corePath[MAX_PATH];
        sprintf_s(corePath, "%s\\OxoSteamCore.dll", selfPath);
        HMODULE existing = GetModuleHandleA("OxoSteamCore.dll");
        if (!existing) {
            g_coreHook = LoadLibraryA(corePath);
            if (g_coreHook) {
                typedef void (*InitFunc)();
                InitFunc init = (InitFunc)GetProcAddress(g_coreHook, "OxoInitialize");
                if (init) init();
            }
        }
    } else if (fdwReason == DLL_PROCESS_DETACH) {
        if (g_coreHook) FreeLibrary(g_coreHook);
        if (g_systemXInput) FreeLibrary(g_systemXInput);
    }
    return TRUE;
}
