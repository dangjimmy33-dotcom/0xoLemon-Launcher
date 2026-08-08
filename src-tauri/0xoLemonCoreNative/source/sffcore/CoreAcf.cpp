#include "sffcore/CoreAcf.h"
#include "sffcore/KeyValues.h"

#include <algorithm>
#include <charconv>
#include <fstream>
#include <set>

namespace SffCore::Acf {
namespace {

std::optional<uint32_t> ParseU32(const std::optional<std::string>& value) {
    if (!value || value->empty()) return std::nullopt;
    uint32_t out = 0;
    const char* begin = value->data();
    const char* end = value->data() + value->size();
    auto result = std::from_chars(begin, end, out);
    if (result.ec != std::errc{} || result.ptr != end) return std::nullopt;
    return out;
}

bool SamePath(const std::filesystem::path& a, const std::filesystem::path& b) {
    std::error_code ec1, ec2;
    auto ca = std::filesystem::weakly_canonical(a, ec1);
    auto cb = std::filesystem::weakly_canonical(b, ec2);
    if (!ec1 && !ec2) return ca == cb;
    return a.lexically_normal() == b.lexically_normal();
}

KeyValues::Node* LibraryFoldersNode(KeyValues::Node& doc) {
    return doc.Find("libraryfolders");
}

} // namespace

std::vector<std::filesystem::path> GetSteamLibraries(const std::filesystem::path& steamPath) {
    std::vector<std::filesystem::path> out;
    std::set<std::filesystem::path> seen;
    const auto vdfPath = steamPath / "config" / "libraryfolders.vdf";
    auto doc = KeyValues::Load(vdfPath.string());
    if (doc) {
        const auto* folders = doc->Find("libraryfolders");
        if (folders && folders->IsObject()) {
            for (const auto& [key, node] : folders->Children()) {
                if (key == "contentstatsid" || !node.IsObject()) continue;
                auto pathString = node.GetString("path");
                if (!pathString || pathString->empty()) continue;
                std::filesystem::path p(*pathString);
                std::error_code ec;
                if (std::filesystem::exists(p, ec) && seen.insert(p.lexically_normal()).second)
                    out.push_back(std::move(p));
            }
        }
    }

    std::error_code ec;
    if (std::filesystem::exists(steamPath, ec)) {
        bool present = false;
        for (const auto& p : out) if (SamePath(p, steamPath)) { present = true; break; }
        if (!present) out.insert(out.begin(), steamPath);
    }
    return out;
}

std::optional<ParsedAcf> ParseFile(const std::filesystem::path& acfPath,
                                   const std::filesystem::path& libraryRoot) {
    auto doc = KeyValues::Load(acfPath.string());
    if (!doc) return std::nullopt;
    const auto* app = doc->Find("AppState");
    if (!app || !app->IsObject()) return std::nullopt;

    ParsedAcf parsed;
    parsed.path = acfPath;
    parsed.libraryRoot = libraryRoot;
    parsed.info.appId = ParseU32(app->GetString("appid")).value_or(0);
    parsed.info.name = app->GetString("name").value_or("");
    parsed.info.stateFlags = ParseU32(app->GetString("StateFlags")).value_or(0);
    parsed.info.installDir = app->GetString("installdir").value_or("");
    if (const auto* depots = app->Find("MountedDepots"); depots && depots->IsObject()) {
        for (const auto& [depotId, manifest] : depots->Children()) {
            if (!manifest.IsObject()) parsed.info.mountedDepots.insert_or_assign(depotId, manifest.Value());
        }
    }
    return parsed;
}

std::optional<ParsedAcf> FindAndParse(const std::filesystem::path& steamPath, uint32_t appId) {
    for (const auto& lib : GetSteamLibraries(steamPath)) {
        const auto acfPath = lib / "steamapps" / ("appmanifest_" + std::to_string(appId) + ".acf");
        std::error_code ec;
        if (!std::filesystem::exists(acfPath, ec)) continue;
        auto parsed = ParseFile(acfPath, lib);
        if (parsed) return parsed;
    }
    return std::nullopt;
}

std::string GetAppName(const std::filesystem::path& steamPath, uint32_t appId) {
    auto parsed = FindAndParse(steamPath, appId);
    return parsed && !parsed->info.name.empty() ? parsed->info.name : std::to_string(appId);
}

bool EnsureLibraryHasApp(const std::filesystem::path& steamPath,
                         const std::filesystem::path& libraryPath,
                         uint32_t appId) {
    const auto vdfPath = steamPath / "config" / "libraryfolders.vdf";
    auto doc = KeyValues::Load(vdfPath.string());
    if (!doc) return false;
    auto* folders = LibraryFoldersNode(*doc);
    if (!folders || !folders->IsObject()) return false;

    KeyValues::Node* target = nullptr;
    uint32_t maxIndex = 0;
    bool haveNumeric = false;
    for (auto& [key, node] : folders->Children()) {
        uint32_t index = 0;
        auto fc = std::from_chars(key.data(), key.data() + key.size(), index);
        if (fc.ec == std::errc{} && fc.ptr == key.data() + key.size()) {
            maxIndex = haveNumeric ? (std::max)(maxIndex, index) : index;
            haveNumeric = true;
        }
        if (!node.IsObject()) continue;
        auto p = node.GetString("path");
        if (p && SamePath(*p, libraryPath)) target = &node;
    }

    if (!target) {
        KeyValues::Node::Object apps;
        KeyValues::Node::Object lib;
        lib.emplace("path", KeyValues::Node(std::filesystem::absolute(libraryPath).lexically_normal().string()));
        lib.emplace("apps", KeyValues::Node(std::move(apps)));
        const std::string key = std::to_string(haveNumeric ? maxIndex + 1 : 0);
        folders->Children().insert_or_assign(key, KeyValues::Node(std::move(lib)));
        target = folders->Find(key);
    }
    if (!target) return false;

    auto* apps = target->Find("apps");
    if (!apps || !apps->IsObject()) {
        target->Children().insert_or_assign("apps", KeyValues::Node(KeyValues::Node::Object{}));
        apps = target->Find("apps");
    }
    if (!apps) return false;
    const std::string id = std::to_string(appId);
    auto existing = apps->GetString(id);
    if (existing && *existing == "1") return false;
    apps->Children().insert_or_assign(id, KeyValues::Node("1"));
    return KeyValues::SaveAtomic(vdfPath.string(), *doc);
}

} // namespace SffCore::Acf
