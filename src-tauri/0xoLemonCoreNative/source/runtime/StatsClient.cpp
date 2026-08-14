// _0xoLemonCore - asynchronous OpenSteamTool stats lookup.

#include "StatsClient.h"
#include "StatsResponse.h"

#include "config/Settings.h"
#include "runtime/Logger.h"
#include "runtime/RuntimeHttp.h"

#include <chrono>
#include <condition_variable>
#include <deque>
#include <mutex>
#include <string>
#include <thread>
#include <unordered_map>
#include <unordered_set>

namespace StatsClient {
namespace {
    using Clock = std::chrono::steady_clock;

    constexpr std::size_t kMaxQueue = 128;
    constexpr std::uint32_t kTimeoutMs = 5'000;
    constexpr std::size_t kBodyCap = 32;

    struct CacheEntry {
        std::uint64_t steamId = 0;
        Clock::time_point expiresAt{};
    };

    // Steam owns the lifetime of this DLL. Keeping the tiny worker state alive
    // until process exit avoids static-destruction races during shutdown.
    struct State {
        std::mutex mutex;
        std::condition_variable wake;
        std::deque<AppId_t> queue;
        std::unordered_set<AppId_t> queued;
        std::unordered_set<AppId_t> inFlight;
        std::unordered_map<AppId_t, CacheEntry> cache;
        bool workerStarted = false;
    };

    State& GetState() {
        static State* state = new State();
        return *state;
    }

    CacheEntry Fetch(AppId_t appId) {
        CacheEntry entry{};

        const std::string url = "https://stats.opensteamtool.com/"
            + std::to_string(appId);
        const auto response = RuntimeHttp::GetLimited(
            url, {}, L"0xoLemonCore-Stats/1.0", kTimeoutMs, kBodyCap);
        const auto parsed = Detail::ClassifyResponse(
            response.networkError, response.status, response.body);
        entry.steamId = parsed.steamId;
        entry.expiresAt = Clock::now() + parsed.ttl;
        if (response.networkError || response.status != 200) {
            LOG_ACHIEVEMENT_WARN(
                "Stats API fallback appid={} status={} networkError={}",
                appId, response.status, response.networkError ? "true" : "false");
            return entry;
        }

        if (entry.steamId == 0) {
            LOG_ACHIEVEMENT_WARN("Stats API returned invalid data appid={} bytes={}",
                                 appId, response.body.size());
            return entry;
        }

        LOG_ACHIEVEMENT_INFO("Stats API resolved appid={} steamid={}",
                             appId, entry.steamId);
        return entry;
    }

    void Worker() {
        State& state = GetState();
        for (;;) {
            AppId_t appId = k_uAppIdInvalid;
            {
                std::unique_lock lock(state.mutex);
                state.wake.wait(lock, [&] { return !state.queue.empty(); });
                appId = state.queue.front();
                state.queue.pop_front();
                state.queued.erase(appId);
                state.inFlight.insert(appId);
            }

            CacheEntry entry = Fetch(appId);
            {
                std::lock_guard lock(state.mutex);
                state.cache[appId] = entry;
                state.inFlight.erase(appId);
            }
        }
    }

    void EnsureWorker(State& state) {
        if (state.workerStarted) return;
        state.workerStarted = true;
        std::thread(Worker).detach();
    }
}

void Schedule(AppId_t appId) {
    if (!Settings::statsEnableApi || appId == k_uAppIdInvalid) return;

    State& state = GetState();
    std::lock_guard lock(state.mutex);
    const auto now = Clock::now();
    auto cached = state.cache.find(appId);
    if (cached != state.cache.end()) {
        if (Detail::IsFresh(cached->second.expiresAt, now)) return;
        state.cache.erase(cached);
    }
    if (!Detail::CanEnqueue(
            state.queued.count(appId) != 0,
            state.inFlight.count(appId) != 0,
            state.queue.size(),
            kMaxQueue)) {
        return;
    }
    EnsureWorker(state);
    state.queue.push_back(appId);
    state.queued.insert(appId);
    state.wake.notify_one();
}

bool TryGet(AppId_t appId, std::uint64_t& steamId) {
    if (!Settings::statsEnableApi || appId == k_uAppIdInvalid) return false;

    State& state = GetState();
    {
        std::lock_guard lock(state.mutex);
        auto cached = state.cache.find(appId);
        if (cached != state.cache.end()
            && Detail::IsFresh(cached->second.expiresAt, Clock::now())) {
            if (cached->second.steamId == 0) return false;
            steamId = cached->second.steamId;
            return true;
        }
        if (cached != state.cache.end()) state.cache.erase(cached);
    }
    Schedule(appId);
    return false;
}

void Forget(AppId_t appId) {
    State& state = GetState();
    std::lock_guard lock(state.mutex);
    state.cache.erase(appId);
}
}
