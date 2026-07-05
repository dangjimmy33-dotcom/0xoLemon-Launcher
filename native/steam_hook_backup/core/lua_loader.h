#pragma once
#include "steam_types.h"
#include <vector>
#include <string>
#include <unordered_set>
#include <unordered_map>
#include <mutex>

namespace OxoLua {

    // Initialize Lua system and start file watcher
    void Initialize(const char* steamPath);
    
    // Shutdown and cleanup
    void Shutdown();

    // Check if an AppID is tracked in lua files
    bool HasDepot(AppId_t appId);

    // Check if an AppID is genuinely owned (to exclude from injection)
    bool IsOwned(AppId_t appId);

    // Mark an AppID as genuinely owned
    void MarkOwned(AppId_t appId);

    // Get all AppIDs from all lua files (for Package 0 injection)
    std::vector<AppId_t> GetAllDepotIds();

    // Get library-visible AppIDs (for GetSubscribedApps)
    std::vector<AppId_t> GetLibraryAppIds();

    // Get depot decryption key for an AppID
    std::vector<uint8_t> GetDecryptionKey(AppId_t depotId);

    // Parse a single lua file
    void ParseFile(const std::string& filePath);

    // Remove an AppID from tracking
    void UnloadFile(const std::string& filePath);

    // Internal state
    struct DepotInfo {
        AppId_t appId;
        std::vector<uint8_t> decryptionKey;
        std::vector<AppId_t> dlcs;
        bool showInLibrary;
    };

    extern std::unordered_map<AppId_t, DepotInfo> g_depots;
    extern std::unordered_set<AppId_t> g_ownedApps;
    extern std::mutex g_mutex;

} // namespace OxoLua
