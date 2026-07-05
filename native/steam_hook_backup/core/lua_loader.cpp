#include "lua_loader.h"
#include <lua.hpp>
#include <Windows.h>
#include <algorithm>
#include <fstream>
#include <sstream>

namespace OxoLua {

std::unordered_map<AppId_t, DepotInfo> g_depots;
std::unordered_set<AppId_t> g_ownedApps;
std::mutex g_mutex;

static std::string g_steamPath;
static HANDLE g_watchThread = nullptr;
static bool g_running = false;

// Lua C functions to capture data
static int lua_addappid(lua_State* L) {
    AppId_t appId = (AppId_t)luaL_checkinteger(L, 1);
    
    char msg[256];
    sprintf_s(msg, "[OxoHook] lua_addappid(%u)\n", appId);
    OutputDebugStringA(msg);
    
    std::lock_guard<std::mutex> lock(g_mutex);
    if (g_depots.find(appId) == g_depots.end()) {
        g_depots[appId] = DepotInfo{};
        g_depots[appId].appId = appId;
        g_depots[appId].showInLibrary = true;
    }
    return 0;
}

static int lua_adddepot(lua_State* L) {
    AppId_t depotId = (AppId_t)luaL_checkinteger(L, 1);
    const char* keyHex = luaL_checkstring(L, 2);
    
    // Parse hex key
    std::vector<uint8_t> key;
    size_t len = strlen(keyHex);
    for (size_t i = 0; i < len; i += 2) {
        char byteStr[3] = {keyHex[i], keyHex[i+1], '\0'};
        key.push_back((uint8_t)strtol(byteStr, nullptr, 16));
    }
    
    std::lock_guard<std::mutex> lock(g_mutex);
    if (g_depots.find(depotId) == g_depots.end()) {
        g_depots[depotId] = DepotInfo{};
        g_depots[depotId].appId = depotId;
    }
    g_depots[depotId].decryptionKey = key;
    return 0;
}

static int lua_adddlc(lua_State* L) {
    AppId_t appId = (AppId_t)luaL_checkinteger(L, 1);
    AppId_t dlcId = (AppId_t)luaL_checkinteger(L, 2);
    
    std::lock_guard<std::mutex> lock(g_mutex);
    if (g_depots.find(appId) != g_depots.end()) {
        g_depots[appId].dlcs.push_back(dlcId);
    }
    return 0;
}

void ParseFile(const std::string& filePath) {
    lua_State* L = luaL_newstate();
    if (!L) return;
    
    luaL_openlibs(L);
    
    // Register our capture functions
    lua_register(L, "addappid", lua_addappid);
    lua_register(L, "adddepot", lua_adddepot);
    lua_register(L, "adddlc", lua_adddlc);
    
    // Execute lua file
    if (luaL_dofile(L, filePath.c_str()) != LUA_OK) {
        const char* err = lua_tostring(L, -1);
        OutputDebugStringA("[OxoHook] Lua parse error: ");
        OutputDebugStringA(err);
        OutputDebugStringA("\n");
    }
    
    lua_close(L);
}

void UnloadFile(const std::string& filePath) {
    // TODO: Track which AppIDs came from which file
    // For now, just clear all
    std::lock_guard<std::mutex> lock(g_mutex);
    g_depots.clear();
}

DWORD WINAPI WatcherThread(LPVOID param) {
    std::string watchDir = g_steamPath + "\\config\\stplug-in";
    
    // Create directory if not exists
    CreateDirectoryA(watchDir.c_str(), nullptr);
    
    HANDLE hDir = CreateFileA(
        watchDir.c_str(),
        FILE_LIST_DIRECTORY,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        nullptr,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS,
        nullptr
    );
    
    if (hDir == INVALID_HANDLE_VALUE) return 0;
    
    char buffer[1024];
    DWORD bytesReturned;
    
    // Initial scan
    OutputDebugStringA("[OxoHook] Scanning for existing .lua files...\n");
    WIN32_FIND_DATAA findData;
    std::string pattern = watchDir + "\\*.lua";
    HANDLE hFind = FindFirstFileA(pattern.c_str(), &findData);
    if (hFind != INVALID_HANDLE_VALUE) {
        int fileCount = 0;
        do {
            std::string fullPath = watchDir + "\\" + findData.cFileName;
            char msg[512];
            sprintf_s(msg, "[OxoHook] Found lua file: %s\n", findData.cFileName);
            OutputDebugStringA(msg);
            ParseFile(fullPath);
            fileCount++;
        } while (FindNextFileA(hFind, &findData));
        FindClose(hFind);
        
        char msg[256];
        sprintf_s(msg, "[OxoHook] Loaded %d lua files\n", fileCount);
        OutputDebugStringA(msg);
    } else {
        OutputDebugStringA("[OxoHook] No lua files found\n");
    }
    
    // Watch for changes
    while (g_running) {
        if (ReadDirectoryChangesW(
            hDir,
            buffer,
            sizeof(buffer),
            FALSE,
            FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE,
            &bytesReturned,
            nullptr,
            nullptr
        )) {
            FILE_NOTIFY_INFORMATION* fni = (FILE_NOTIFY_INFORMATION*)buffer;
            do {
                // Convert wide filename to narrow
                char filename[MAX_PATH];
                WideCharToMultiByte(CP_ACP, 0, fni->FileName, fni->FileNameLength / 2, 
                                   filename, MAX_PATH, nullptr, nullptr);
                filename[fni->FileNameLength / 2] = '\0';
                
                // Check if .lua file
                if (strstr(filename, ".lua")) {
                    std::string fullPath = watchDir + "\\" + filename;
                    
                    if (fni->Action == FILE_ACTION_ADDED || fni->Action == FILE_ACTION_MODIFIED) {
                        Sleep(100); // Wait for file write to complete
                        ParseFile(fullPath);
                    } else if (fni->Action == FILE_ACTION_REMOVED) {
                        UnloadFile(fullPath);
                    }
                }
                
                if (fni->NextEntryOffset == 0) break;
                fni = (FILE_NOTIFY_INFORMATION*)((BYTE*)fni + fni->NextEntryOffset);
            } while (true);
        }
    }
    
    CloseHandle(hDir);
    return 0;
}

void Initialize(const char* steamPath) {
    g_steamPath = steamPath;
    g_running = true;
    
    char msg[512];
    sprintf_s(msg, "[OxoHook] Lua loader initialized - watching: %s\\config\\stplug-in\n", steamPath);
    OutputDebugStringA(msg);
    
    g_watchThread = CreateThread(nullptr, 0, WatcherThread, nullptr, 0, nullptr);
}

void Shutdown() {
    g_running = false;
    if (g_watchThread) {
        WaitForSingleObject(g_watchThread, 2000);
        CloseHandle(g_watchThread);
    }
}

bool HasDepot(AppId_t appId) {
    std::lock_guard<std::mutex> lock(g_mutex);
    if (g_ownedApps.count(appId)) return false; // Actually owned
    return g_depots.count(appId) > 0;
}

bool IsOwned(AppId_t appId) {
    std::lock_guard<std::mutex> lock(g_mutex);
    return g_ownedApps.count(appId) > 0;
}

void MarkOwned(AppId_t appId) {
    std::lock_guard<std::mutex> lock(g_mutex);
    g_ownedApps.insert(appId);
}

std::vector<AppId_t> GetAllDepotIds() {
    std::lock_guard<std::mutex> lock(g_mutex);
    std::vector<AppId_t> result;
    for (const auto& pair : g_depots) {
        if (!g_ownedApps.count(pair.first)) {
            result.push_back(pair.first);
        }
    }
    return result;
}

std::vector<AppId_t> GetLibraryAppIds() {
    std::lock_guard<std::mutex> lock(g_mutex);
    std::vector<AppId_t> result;
    for (const auto& pair : g_depots) {
        if (pair.second.showInLibrary && !g_ownedApps.count(pair.first)) {
            result.push_back(pair.first);
        }
    }
    return result;
}

std::vector<uint8_t> GetDecryptionKey(AppId_t depotId) {
    std::lock_guard<std::mutex> lock(g_mutex);
    auto it = g_depots.find(depotId);
    if (it != g_depots.end()) {
        return it->second.decryptionKey;
    }
    return {};
}

} // namespace OxoLua
