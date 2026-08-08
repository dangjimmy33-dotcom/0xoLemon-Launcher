#include "sffcore/KeyValues.h"
#include "sffcore/CoreIni.h"
#include "sffcore/CoreCache.h"
#include "sffcore/CoreAcf.h"
#include "sffcore/CoreNamedIds.h"
#include "sffcore/CoreSettings.h"
#include "sffcore/CoreIntegrity.h"

#include <cassert>
#include <chrono>
#include <filesystem>
#include <fstream>
#include <string>

int main() {
    using namespace SffCore;

    {
        const std::string text = R"VDF("AppState"
{
    "appid" "123"
    "name" "Example Game"
    "MountedDepots"
    {
        "10" "111"
    }
})VDF";
        auto doc = KeyValues::Parse(text);
        assert(doc);
        auto* app = doc->Find("AppState");
        assert(app && app->IsObject());
        assert(app->GetString("appid").value_or("") == "123");
        assert(app->GetString("name").value_or("") == "Example Game");
        auto* depots = app->Find("MountedDepots");
        assert(depots && depots->GetString("10").value_or("") == "111");
    }

    {
        const std::string text = R"VDF("libraryfolders" { "0" { "path" "C:\Steam\Library" } })VDF";
        auto doc = KeyValues::Parse(text);
        assert(doc);
        auto* folders = doc->Find("libraryfolders");
        assert(folders);
        auto* zero = folders->Find("0");
        assert(zero);
        assert(zero->GetString("path").value_or("") == R"(C:\Steam\Library)");
    }

    const auto root = std::filesystem::temp_directory_path() / "oxo_sffcore_portable_test";
    std::filesystem::remove_all(root);
    std::filesystem::create_directories(root);

    {
        const auto ini = root / "sample.ini";
        std::ofstream(ini) << "; comment\n[steam]\nbranch = public\nother=keep\n";
        auto changed = Ini::EditOption(ini, "steam", "branch", [](std::string_view current) {
            assert(current == "public");
            return std::string("beta");
        });
        assert(changed && *changed == "beta");
        std::ifstream in(ini);
        std::string all((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
        assert(all.find("; comment") != std::string::npos);
        assert(all.find("branch = beta") != std::string::npos);
        assert(all.find("other=keep") != std::string::npos);
    }

    {
        Cache cache(root / "cache.bin");
        cache.Set("fresh", "value", std::chrono::seconds(60));
        assert(cache.Get("fresh").value_or("") == "value");
        cache.Set("expired", "gone", std::chrono::seconds(-1));
        assert(!cache.Get("expired").has_value());
        cache.Flush();

        Cache reloaded(root / "cache.bin");
        assert(reloaded.Get("fresh").value_or("") == "value");
        assert(!reloaded.Get("expired").has_value());
    }

    {
        const auto steam = root / "Steam";
        std::filesystem::create_directories(steam / "config");
        std::filesystem::create_directories(steam / "steamapps");
        std::ofstream(steam / "config" / "libraryfolders.vdf") << R"VDF("libraryfolders"
{
    "0"
    {
        "path" ")VDF" << steam.string() << R"VDF("
        "apps" { "123" "1" }
    }
})VDF";
        std::ofstream(steam / "steamapps" / "appmanifest_123.acf") << R"VDF("AppState"
{
    "appid" "123"
    "name" "Native Test Game"
    "StateFlags" "6"
    "installdir" "NativeTest"
    "MountedDepots" { "10" "111" }
})VDF";
        auto acf = Acf::FindAndParse(steam, 123);
        assert(acf);
        assert(acf->info.appId == 123);
        assert(acf->info.name == "Native Test Game");
        assert(acf->info.NeedsUpdate());
        assert(acf->info.mountedDepots.at("10") == "111");
    }

    {
        const auto manifest = root / "sample.manifest";
        std::ofstream out(manifest, std::ios::binary);
        const unsigned char bytes[24] = {0x27, 0x44, 0x56, 0x01};
        out.write(reinterpret_cast<const char*>(bytes), sizeof(bytes));
        out.close();
        assert(Integrity::VerifyManifestMagic(manifest));
        assert(Integrity::VerifyManifestParseable(manifest));
        assert(Integrity::VerifyFileSize(manifest, 24));
    }

    {
        const auto saved = root / "saved_lua";
        std::filesystem::create_directories(saved);
        std::ofstream(saved / "480.lua") << "-- test";
        auto ids = NamedIds::Load(saved);
        assert(ids.at("480") == "480");
        assert(std::filesystem::exists(saved / "names.json"));
    }

    {
        SettingsStore settings(root / "settings.toml");
        settings.Set("advanced_mode", true);
        settings.Set("steam_path", std::string("C:/Steam"));
        assert(settings.Flush());
        SettingsStore loaded(root / "settings.toml");
        assert(loaded.GetBool("advanced_mode").value_or(false));
        assert(loaded.GetString("steam_path").value_or("") == "C:/Steam");
    }

    std::filesystem::remove_all(root);
    return 0;
}
