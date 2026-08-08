#pragma once

#include <chrono>
#include <filesystem>
#include <mutex>
#include <optional>
#include <string>
#include <string_view>
#include <unordered_map>

namespace SffCore {

class Cache {
public:
    explicit Cache(std::filesystem::path path);
    std::optional<std::string> Get(std::string_view key);
    void Set(std::string key, std::string value, std::chrono::seconds ttl = std::chrono::hours(1));
    void Invalidate(std::optional<std::string_view> key = std::nullopt);
    size_t CleanupExpired();
    bool Flush();

private:
    struct Entry {
        std::string value;
        std::chrono::system_clock::time_point expires;
    };

    void Load();
    bool FlushUnlocked();

    std::filesystem::path path_;
    std::unordered_map<std::string, Entry> entries_;
    std::mutex mutex_;
    bool dirty_ = false;
};

} // namespace SffCore
