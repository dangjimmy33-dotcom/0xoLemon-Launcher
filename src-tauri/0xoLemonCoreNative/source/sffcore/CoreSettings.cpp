#include "sffcore/CoreSettings.h"

#include <charconv>
#include <cctype>
#include <fstream>
#include <sstream>

namespace SffCore {
namespace {
std::string_view Trim(std::string_view s) {
    while (!s.empty() && std::isspace(static_cast<unsigned char>(s.front()))) s.remove_prefix(1);
    while (!s.empty() && std::isspace(static_cast<unsigned char>(s.back()))) s.remove_suffix(1);
    return s;
}

std::string Escape(std::string_view s) {
    std::string out;
    for (char c : s) {
        switch (c) {
        case '\\': out += "\\\\"; break;
        case '"': out += "\\\""; break;
        case '\n': out += "\\n"; break;
        case '\r': out += "\\r"; break;
        case '\t': out += "\\t"; break;
        default: out.push_back(c); break;
        }
    }
    return out;
}

std::optional<std::string> Unquote(std::string_view s) {
    s = Trim(s);
    if (s.size() < 2 || s.front() != '"' || s.back() != '"') return std::nullopt;
    std::string out;
    for (size_t i = 1; i + 1 < s.size(); ++i) {
        char c = s[i];
        if (c == '\\' && i + 2 < s.size()) {
            char e = s[++i];
            switch (e) {
            case 'n': out.push_back('\n'); break;
            case 'r': out.push_back('\r'); break;
            case 't': out.push_back('\t'); break;
            case '\\': out.push_back('\\'); break;
            case '"': out.push_back('"'); break;
            default: out.push_back(e); break;
            }
        } else out.push_back(c);
    }
    return out;
}
} // namespace

SettingsStore::SettingsStore(std::filesystem::path path) : path_(std::move(path)) { Load(); }

void SettingsStore::Load() {
    std::lock_guard lock(mutex_);
    std::ifstream in(path_);
    if (!in) return;
    std::string line;
    while (std::getline(in, line)) {
        auto view = Trim(line);
        if (view.empty() || view.front() == '#' || view.front() == ';' || view.front() == '[') continue;
        const auto eq = view.find('=');
        if (eq == std::string_view::npos) continue;
        std::string key(Trim(view.substr(0, eq)));
        auto raw = Trim(view.substr(eq + 1));
        if (key.empty()) continue;
        if (auto s = Unquote(raw)) values_[std::move(key)] = std::move(*s);
        else if (raw == "true") values_[std::move(key)] = true;
        else if (raw == "false") values_[std::move(key)] = false;
        else {
            int64_t i = 0;
            auto result = std::from_chars(raw.data(), raw.data() + raw.size(), i);
            if (result.ec == std::errc{} && result.ptr == raw.data() + raw.size()) values_[std::move(key)] = i;
        }
    }
    dirty_ = false;
}

std::optional<std::string> SettingsStore::GetString(std::string_view key) const {
    std::lock_guard lock(mutex_);
    auto it = values_.find(key);
    if (it == values_.end()) return std::nullopt;
    if (auto p = std::get_if<std::string>(&it->second)) return *p;
    return std::nullopt;
}

std::optional<bool> SettingsStore::GetBool(std::string_view key) const {
    std::lock_guard lock(mutex_);
    auto it = values_.find(key);
    if (it == values_.end()) return std::nullopt;
    if (auto p = std::get_if<bool>(&it->second)) return *p;
    return std::nullopt;
}

std::optional<int64_t> SettingsStore::GetInt(std::string_view key) const {
    std::lock_guard lock(mutex_);
    auto it = values_.find(key);
    if (it == values_.end()) return std::nullopt;
    if (auto p = std::get_if<int64_t>(&it->second)) return *p;
    return std::nullopt;
}

void SettingsStore::Set(std::string key, std::string value) { std::lock_guard lock(mutex_); values_[std::move(key)] = std::move(value); dirty_ = true; }
void SettingsStore::Set(std::string key, bool value) { std::lock_guard lock(mutex_); values_[std::move(key)] = value; dirty_ = true; }
void SettingsStore::Set(std::string key, int64_t value) { std::lock_guard lock(mutex_); values_[std::move(key)] = value; dirty_ = true; }

bool SettingsStore::Clear(std::string_view key) {
    std::lock_guard lock(mutex_);
    const bool erased = values_.erase(std::string(key)) != 0;
    dirty_ |= erased;
    return erased;
}

bool SettingsStore::Flush() {
    std::lock_guard lock(mutex_);
    if (!dirty_) return true;
    std::error_code ec;
    if (path_.has_parent_path()) std::filesystem::create_directories(path_.parent_path(), ec);
    auto tmp = path_;
    tmp += ".tmp";
    std::ofstream out(tmp, std::ios::trunc);
    if (!out) return false;
    out << "# 0xoLemon native core settings\n";
    for (const auto& [key, value] : values_) {
        out << key << " = ";
        if (const auto* p = std::get_if<std::string>(&value)) out << '"' << Escape(*p) << '"';
        else if (const auto* p = std::get_if<bool>(&value)) out << (*p ? "true" : "false");
        else out << std::get<int64_t>(value);
        out << '\n';
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
