#include "sffcore/CoreNamedIds.h"

#include <cctype>
#include <fstream>
#include <sstream>

namespace SffCore::NamedIds {
namespace {
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

void SkipWs(std::string_view s, size_t& p) {
    while (p < s.size() && std::isspace(static_cast<unsigned char>(s[p]))) ++p;
}

bool ParseString(std::string_view s, size_t& p, std::string& out) {
    SkipWs(s, p);
    if (p >= s.size() || s[p++] != '"') return false;
    out.clear();
    while (p < s.size()) {
        char c = s[p++];
        if (c == '"') return true;
        if (c == '\\' && p < s.size()) {
            char e = s[p++];
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
    return false;
}

Registry Read(const std::filesystem::path& file) {
    Registry out;
    std::ifstream in(file, std::ios::binary);
    if (!in) return out;
    const std::string text((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
    size_t p = 0;
    SkipWs(text, p);
    if (p >= text.size() || text[p++] != '{') return {};
    while (true) {
        SkipWs(text, p);
        if (p < text.size() && text[p] == '}') return out;
        std::string key, value;
        if (!ParseString(text, p, key)) return {};
        SkipWs(text, p);
        if (p >= text.size() || text[p++] != ':') return {};
        if (!ParseString(text, p, value)) return {};
        out.insert_or_assign(std::move(key), std::move(value));
        SkipWs(text, p);
        if (p < text.size() && text[p] == ',') { ++p; continue; }
        if (p < text.size() && text[p] == '}') return out;
        return {};
    }
}
} // namespace

bool Save(const std::filesystem::path& folder, const Registry& registry) {
    std::error_code ec;
    std::filesystem::create_directories(folder, ec);
    const auto target = folder / "names.json";
    auto tmp = target;
    tmp += ".tmp";
    std::ofstream out(tmp, std::ios::binary | std::ios::trunc);
    if (!out) return false;
    out << "{\n";
    size_t i = 0;
    for (const auto& [id, name] : registry) {
        out << "  \"" << Escape(id) << "\": \"" << Escape(name) << "\"";
        if (++i < registry.size()) out << ',';
        out << '\n';
    }
    out << "}\n";
    out.close();
    if (!out) return false;
    std::filesystem::rename(tmp, target, ec);
    if (ec) {
        std::filesystem::remove(target, ec);
        ec.clear();
        std::filesystem::rename(tmp, target, ec);
    }
    return !ec;
}

Registry Load(const std::filesystem::path& folder) {
    std::error_code ec;
    std::filesystem::create_directories(folder, ec);
    Registry registry = Read(folder / "names.json");
    bool dirty = false;
    for (std::filesystem::directory_iterator it(folder, ec), end; !ec && it != end; it.increment(ec)) {
        if (!it->is_regular_file(ec) || it->path().extension() != ".lua") continue;
        const std::string id = it->path().stem().string();
        if (!registry.contains(id)) {
            registry[id] = id;
            dirty = true;
        }
    }
    if (dirty || !std::filesystem::exists(folder / "names.json", ec)) Save(folder, registry);
    return registry;
}

} // namespace SffCore::NamedIds
