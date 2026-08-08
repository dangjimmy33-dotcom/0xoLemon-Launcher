#pragma once

#include <filesystem>
#include <map>
#include <mutex>
#include <optional>
#include <string>
#include <string_view>
#include <variant>

namespace SffCore {

class SettingsStore {
public:
    using Value = std::variant<std::string, bool, int64_t>;

    explicit SettingsStore(std::filesystem::path path);

    std::optional<std::string> GetString(std::string_view key) const;
    std::optional<bool> GetBool(std::string_view key) const;
    std::optional<int64_t> GetInt(std::string_view key) const;

    void Set(std::string key, std::string value);
    void Set(std::string key, bool value);
    void Set(std::string key, int64_t value);
    bool Clear(std::string_view key);
    bool Flush();

private:
    void Load();
    std::filesystem::path path_;
    mutable std::mutex mutex_;
    std::map<std::string, Value, std::less<>> values_;
    bool dirty_ = false;
};

} // namespace SffCore
