#include "steam_hook.h"
#include <MinHook.h>
#include <stdio.h>
#include <string>

// We try to hook the flat API if exported
typedef bool(__cdecl* SetAchievement_t)(intptr_t instance, const char* pchName);
SetAchievement_t pOriginalSetAchievement = nullptr;

bool __cdecl Hooked_SetAchievement(intptr_t instance, const char* pchName) {
    if (pchName) {
        // Send to launcher over TCP
        // (Just a simple stub for now, layer 2 and 3 are the primary fallbacks)
        char dbgMsg[256];
        snprintf(dbgMsg, sizeof(dbgMsg), "[Overlay] Achievement Unlocked: %s\n", pchName);
        OutputDebugStringA(dbgMsg);
    }
    return pOriginalSetAchievement(instance, pchName);
}

namespace SteamHook {
    void Initialize() {
        HMODULE hSteamApi = GetModuleHandleA("steam_api64.dll");
        if (!hSteamApi) hSteamApi = GetModuleHandleA("steam_api.dll");
        
        if (hSteamApi) {
            void* pSetAchievement = GetProcAddress(hSteamApi, "SteamAPI_ISteamUserStats_SetAchievement");
            if (pSetAchievement) {
                if (MH_CreateHook(pSetAchievement, &Hooked_SetAchievement, (LPVOID*)&pOriginalSetAchievement) == MH_OK) {
                    MH_EnableHook(pSetAchievement);
                }
            }
        }
    }

    void Uninitialize() {
        HMODULE hSteamApi = GetModuleHandleA("steam_api64.dll");
        if (!hSteamApi) hSteamApi = GetModuleHandleA("steam_api.dll");
        
        if (hSteamApi) {
            void* pSetAchievement = GetProcAddress(hSteamApi, "SteamAPI_ISteamUserStats_SetAchievement");
            if (pSetAchievement) {
                MH_DisableHook(pSetAchievement);
                MH_RemoveHook(pSetAchievement);
            }
        }
    }
}
