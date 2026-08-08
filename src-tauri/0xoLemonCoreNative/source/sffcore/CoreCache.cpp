#include "sffcore/CoreCache.h"

#include <cstdint>
#include <fstream>
#include <limits>

namespace SffCore {
namespace {
constexpr char kMagic[] = "OXOCACHE1";

template <typename T>
bool ReadPod(std::istream& in, T& value) {
    return static_cast<bool>(in.read(reinterpret_cast<char*>(&value), sizeof(value)));
}

template <typename T>
void WritePod(std::ostream& out, T value) {
    out.write(reinterpret_cast<const char*>(&value), sizeof(value));
}
} // namespace

Cache::Cache(std::filesystem::path path) : path_(std::move(path)) { Load(); }

void Cache::Load() {
    std::lock_guard lock(mutex_);
    std::ifstream in(path_, std::ios::binary);
    if (!in) return;
    char magic[sizeof(kMagic)]{};
    if (!in.read(magic, sizeof(magic)) || std::string_view(magic, sizeof(magic)) != std::string_view(kMagic, sizeof(kMagic))) return;
    uint32_t count = 0;
    if (!ReadPod(in, count) || count > 100000) return;
    for (uint32_t i = 0; i < count; ++i) {
        uint32_t keySize = 0, valueSize = 0;
        int64_t expiresSeconds = 0;
        if (!ReadPod(in, keySize) || !ReadPod(in, valueSize) || !ReadPod(in, expiresSeconds)) return;
        if (keySize > 1024 * 1024 || valueSize > 64 * 1024 * 1024) return;
        std::string key(keySize, '\0'), value(valueSize, '\0');
        if (!in.read(key.data(), static_cast<std::streamsize>(key.size()))) return;
        if (!in.read(value.data(), static_cast<std::streamsize>(value.size()))) return;
        entries_[std::move(key)] = Entry{std::move(value), std::chrono::system_clock::time_point(std::chrono::seconds(expiresSeconds))};
    }
    dirty_ = false;
}

std::optional<std::string> Cache::Get(std::string_view key) {
    std::lock_guard lock(mutex_);
    auto it = entries_.find(std::string(key));
    if (it == entries_.end()) return std::nullopt;
    if (std::chrono::system_clock::now() > it->second.expires) {
        entries_.erase(it);
        dirty_ = true;
        return std::nullopt;
    }
    return it->second.value;
}

void Cache::Set(std::string key, std::string value, std::chrono::seconds ttl) {
    std::lock_guard lock(mutex_);
    entries_[std::move(key)] = Entry{std::move(value), std::chrono::system_clock::now() + ttl};
    dirty_ = true;
}

void Cache::Invalidate(std::optional<std::string_view> key) {
    std::lock_guard lock(mutex_);
    if (key) entries_.erase(std::string(*key));
    else entries_.clear();
    dirty_ = true;
}

size_t Cache::CleanupExpired() {
    std::lock_guard lock(mutex_);
    const auto now = std::chrono::system_clock::now();
    size_t removed = 0;
    for (auto it = entries_.begin(); it != entries_.end();) {
        if (now > it->second.expires) {
            it = entries_.erase(it);
            ++removed;
        } else {
            ++it;
        }
    }
    if (removed) dirty_ = true;
    return removed;
}

bool Cache::Flush() {
    std::lock_guard lock(mutex_);
    return FlushUnlocked();
}

bool Cache::FlushUnlocked() {
    if (!dirty_) return true;
    std::error_code ec;
    if (path_.has_parent_path()) std::filesystem::create_directories(path_.parent_path(), ec);
    auto tmp = path_;
    tmp += ".tmp";
    std::ofstream out(tmp, std::ios::binary | std::ios::trunc);
    if (!out) return false;
    out.write(kMagic, sizeof(kMagic));
    WritePod<uint32_t>(out, static_cast<uint32_t>(entries_.size()));
    for (const auto& [key, entry] : entries_) {
        if (key.size() > std::numeric_limits<uint32_t>::max() || entry.value.size() > std::numeric_limits<uint32_t>::max()) return false;
        const auto expires = std::chrono::duration_cast<std::chrono::seconds>(entry.expires.time_since_epoch()).count();
        WritePod<uint32_t>(out, static_cast<uint32_t>(key.size()));
        WritePod<uint32_t>(out, static_cast<uint32_t>(entry.value.size()));
        WritePod<int64_t>(out, expires);
        out.write(key.data(), static_cast<std::streamsize>(key.size()));
        out.write(entry.value.data(), static_cast<std::streamsize>(entry.value.size()));
    }
    out.close();
    if (!out) return false;
    std::filesystem::rename(tmp, path_, ec);
    if (ec) {
        std::filesystem::remove(path_, ec);
        ec.clear();
        std::filesystem::rename(tmp, path_, ec);
    }
    if (ec) return false;
    dirty_ = false;
    return true;
}

} // namespace SffCore
