#include "sffcore/NativeCore.h"
#include "sffcore/CoreCache.h"
#include "sffcore/CorePaths.h"
#include "sffcore/CoreSettings.h"
#include "runtime/Logger.h"

#include <memory>
#include <mutex>

namespace SffCore {
namespace {
std::mutex g_mutex;
std::unique_ptr<Cache> g_cache;
std::unique_ptr<SettingsStore> g_settings;
bool g_initialized = false;
}

bool Initialize() {
    std::lock_guard lock(g_mutex);
    if (g_initialized) return true;
    std::error_code ec;
    std::filesystem::create_directories(Paths::DataDirectory(), ec);
    std::filesystem::create_directories(Paths::ManifestStagingDirectory(), ec);
    if (ec) return false;

    g_cache = std::make_unique<Cache>(Paths::DataDirectory() / "0xo_api_cache.bin");
    const size_t expired = g_cache->CleanupExpired();
    g_cache->Flush();
    g_settings = std::make_unique<SettingsStore>(Paths::DataDirectory() / "0xo_settings.toml");
    g_initialized = true;
    LOG_INFO("Native SFF core initialized; data={} expired_cache={}", Paths::DataDirectory().string(), expired);
    return true;
}

void Shutdown() {
    std::lock_guard lock(g_mutex);
    if (!g_initialized) return;
    if (g_cache) g_cache->Flush();
    if (g_settings) g_settings->Flush();
    g_settings.reset();
    g_cache.reset();
    g_initialized = false;
}

} // namespace SffCore
