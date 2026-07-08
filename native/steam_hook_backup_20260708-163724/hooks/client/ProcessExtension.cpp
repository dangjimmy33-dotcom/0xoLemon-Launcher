// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "hooks/client/ProcessExtension.h"

#include "core/entry.h"
#include "runtime/Logger.h"
#include "runtime/RemoteTools.h"
#include "config/Settings.h"

#include <windows.h>
#include <tlhelp32.h>

#include <algorithm>
#include <chrono>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <unordered_map>
#include <mutex>
#include <optional>
#include <string>
#include <string_view>
#include <unordered_set>
#include <vector>
#include <cwctype>

namespace {

    struct ProcessKeyHash {
        std::size_t operator()(const PipeWatch::ProcessKey& key) const noexcept {
            return (static_cast<std::size_t>(key.pid) << 1) ^
                   static_cast<std::size_t>(key.creation ^ (key.creation >> 32));
        }
    };

    std::mutex g_lock;
    std::unordered_set<PipeWatch::ProcessKey, ProcessKeyHash> g_seen;

    struct LaunchHint {
        AppId_t appId = k_uAppIdInvalid;
        std::chrono::steady_clock::time_point expires{};
    };

    std::unordered_map<std::string, LaunchHint> g_launchHints;
    constexpr auto kLaunchHintTtl = std::chrono::minutes(5);

    std::wstring Utf8ToWide(const std::string& text) {
        if (text.empty())
            return {};
        int needed = MultiByteToWideChar(CP_UTF8, 0, text.c_str(),
                                         static_cast<int>(text.size()),
                                         nullptr, 0);
        if (needed <= 0)
            return {};
        std::wstring out(static_cast<std::size_t>(needed), L'\0');
        MultiByteToWideChar(CP_UTF8, 0, text.c_str(),
                            static_cast<int>(text.size()),
                            out.data(), needed);
        return out;
    }

    std::string LowerNarrow(std::string value) {
        for (char& ch : value) {
            if (ch >= 'A' && ch <= 'Z')
                ch = static_cast<char>(ch - 'A' + 'a');
        }
        return value;
    }

    std::string FileNameOf(std::string_view path) {
        while (!path.empty() && (path.front() == '"' || path.front() == '\'')) {
            path.remove_prefix(1);
        }
        while (!path.empty() && (path.back() == '"' || path.back() == '\'')) {
            path.remove_suffix(1);
        }
        std::size_t pos = path.find_last_of("\\/");
        std::string name = (pos == std::string_view::npos)
            ? std::string(path)
            : std::string(path.substr(pos + 1));
        return LowerNarrow(std::move(name));
    }

    void PruneLaunchHintsLocked(std::chrono::steady_clock::time_point now) {
        for (auto it = g_launchHints.begin(); it != g_launchHints.end();) {
            if (it->second.expires <= now)
                it = g_launchHints.erase(it);
            else
                ++it;
        }
    }

    AppId_t TakeLaunchHintFor(const PipeWatch::ProcessSnapshot& snapshot) {
        std::string name = FileNameOf(snapshot.imageName.empty()
            ? std::string_view(snapshot.imagePath)
            : std::string_view(snapshot.imageName));
        if (name.empty())
            return k_uAppIdInvalid;

        std::scoped_lock lock(g_lock);
        auto now = std::chrono::steady_clock::now();
        PruneLaunchHintsLocked(now);

        auto it = g_launchHints.find(name);
        if (it == g_launchHints.end())
            return k_uAppIdInvalid;

        AppId_t appId = it->second.appId;
        g_launchHints.erase(it);
        return appId;
    }

    std::string SelectConfiguredLibrary(RemoteTools::ProcessBits bits) {
        if (bits == RemoteTools::ProcessBits::X86)
            return Settings::processExtensionX86;
        if (bits == RemoteTools::ProcessBits::X64)
            return Settings::processExtensionX64;
#if defined(_WIN64)
        return Settings::processExtensionX64;
#else
        return Settings::processExtensionX86;
#endif
    }

    std::filesystem::path ResolveConfiguredLibrary(const std::string& configured) {
        std::filesystem::path path(Utf8ToWide(configured));
        if (path.is_absolute())
            return path;

        std::filesystem::path steamRoot(Utf8ToWide(SteamInstallPath));
        if (steamRoot.empty())
            return path;
        return steamRoot / path;
    }

} // namespace

namespace ProcessExtension {

    void Reset() {
        std::scoped_lock lock(g_lock);
        g_seen.clear();
        g_launchHints.clear();
    }

    void QueueLaunchHint(const char* exePath, AppId_t appId) {
        if (!exePath || !*exePath || appId == k_uAppIdInvalid)
            return;

        std::string key = FileNameOf(exePath);
        if (key.empty())
            return;

        {
            std::scoped_lock lock(g_lock);
            PruneLaunchHintsLocked(std::chrono::steady_clock::now());
            g_launchHints[key] = LaunchHint{
                appId,
                std::chrono::steady_clock::now() + kLaunchHintTtl,
            };
        }

        LOG_IPCCH_DEBUG("ProcessExtension: launch hint queued image={} appid={}",
                        key, appId);
    }

    void OnGamePipe(const PipeWatch::ProcessSnapshot& snapshot) {
        if (!snapshot.key.IsValid())
            return;

        if (!Settings::processExtensionEnabled)
            return;

        AppId_t hintedAppId = TakeLaunchHintFor(snapshot);
        bool hinted = hintedAppId != k_uAppIdInvalid;
        if ((!snapshot.likelyGame || !snapshot.luaManaged) && !hinted)
            return;

        AppId_t effectiveAppId = hinted ? hintedAppId : snapshot.appId;
        if (effectiveAppId == k_uAppIdInvalid)
            return;

        RemoteTools::ProcessBits bits = RemoteTools::DetectBits(snapshot.key.pid);

#if defined(_WIN64)
        if (bits == RemoteTools::ProcessBits::X86) {
            LOG_IPCCH_WARN("ProcessExtension: skipping pid={} appid={} arch=x86 from x64 loader",
                           snapshot.key.pid, effectiveAppId);
            return;
        }
#else
        if (bits == RemoteTools::ProcessBits::X64) {
            LOG_IPCCH_WARN("ProcessExtension: skipping pid={} appid={} arch=x64 from x86 loader",
                           snapshot.key.pid, effectiveAppId);
            return;
        }
#endif

        std::string library = SelectConfiguredLibrary(bits);
        if (library.empty()) {
            LOG_IPCCH_WARN("ProcessExtension: enabled but no {} helper library configured for pid={} appid={}",
                           RemoteTools::BitsName(bits), snapshot.key.pid, effectiveAppId);
            return;
        }

        std::filesystem::path helperPath = ResolveConfiguredLibrary(library);
        if (helperPath.empty() || !std::filesystem::exists(helperPath)) {
            LOG_IPCCH_WARN("ProcessExtension: helper library missing for pid={} appid={} path={}",
                           snapshot.key.pid, effectiveAppId, helperPath.string());
            return;
        }

        {
            std::scoped_lock lock(g_lock);
            if (!g_seen.insert(snapshot.key).second) {
                LOG_IPCCH_DEBUG("ProcessExtension: process already observed pid={} appid={}",
                                snapshot.key.pid, effectiveAppId);
                return;
            }
        }

        RemoteTools::LoadResult loaded = RemoteTools::LoadLibraryInto(snapshot.key.pid, helperPath);
        if (!loaded.ok) {
            LOG_IPCCH_WARN("ProcessExtension: helper injection failed pid={} appid={} arch={} hint={} image={} helper={} error={}",
                           snapshot.key.pid,
                           effectiveAppId,
                           RemoteTools::BitsName(bits),
                           hinted ? "spawn" : "pipe",
                           snapshot.imageName.empty() ? "-" : snapshot.imageName,
                           helperPath.string(),
                           loaded.error);
            return;
        }

        LOG_IPCCH_INFO("ProcessExtension: helper injected pid={} appid={} arch={} hint={} already_loaded={} image={} helper={}",
                       snapshot.key.pid,
                       effectiveAppId,
                       RemoteTools::BitsName(bits),
                       hinted ? "spawn" : "pipe",
                       loaded.alreadyLoaded,
                       snapshot.imageName.empty() ? "-" : snapshot.imageName,
                       helperPath.string());
    }

}


