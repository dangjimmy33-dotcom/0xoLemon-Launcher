// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "hooks/client/AuthWindow.h"

#include "runtime/Logger.h"
#include "runtime/ProtectionProbe.h"
#include "runtime/Ticket.h"

#include <mutex>
#include <optional>
#include <unordered_map>

namespace {

    struct ProcessKeyHash {
        std::size_t operator()(const PipeWatch::ProcessKey& key) const noexcept {
            return (static_cast<std::size_t>(key.pid) << 1) ^
                   static_cast<std::size_t>(key.creation ^ (key.creation >> 32));
        }
    };

    struct PipeKey {
        uint32 pid = 0;
        HSteamPipe pipe = 0;

        bool operator==(const PipeKey&) const = default;
    };

    struct PipeKeyHash {
        std::size_t operator()(const PipeKey& key) const noexcept {
            return (static_cast<std::size_t>(key.pid) << 1) ^
                   static_cast<std::size_t>(key.pipe);
        }
    };

    enum class WindowState {
        None,
        Open,
        Closed,
    };

    struct AuthState {
        bool scanned = false;
        bool protectedGame = false;
        bool steamIdWritten = false;
        uint32 handshakeCount = 0;
        AppId_t appId = k_uAppIdInvalid;
        WindowState window = WindowState::None;
        std::optional<PipeKey> selectedPipe;
        std::string method;
    };

    constexpr uint32 kHandshakeWindow = 2;

    std::mutex g_lock;
    std::unordered_map<PipeWatch::ProcessKey, AuthState, ProcessKeyHash> g_processes;
    std::unordered_map<PipeKey, PipeWatch::ProcessKey, PipeKeyHash> g_pipes;

    PipeKey MakePipeKey(const CSteamPipeClient* pipe) {
        if (!pipe) return {};
        return PipeKey{pipe->m_clientPID, static_cast<HSteamPipe>(pipe->m_hSteamPipe)};
    }

    const char* StateName(WindowState state) {
        switch (state) {
            case WindowState::Open:   return "open";
            case WindowState::Closed: return "closed";
            default:                  return "none";
        }
    }

    void WriteActiveIdentityOnce(AuthState& state, bool ownedByAccount) {
        if (state.steamIdWritten || state.appId == 0 || state.appId == k_uAppIdInvalid)
            return;
        if (!ownedByAccount) {
            LOG_IPCCH_DEBUG("AuthWindow: skip active identity persist appid={} method={} reason=not-owned",
                            state.appId, state.method.empty() ? "-" : state.method);
            return;
        }

        const uint64 steamId = Ticket::GetActiveSteamID64();
        if (steamId == 0) {
            LOG_IPCCH_WARN("AuthWindow: active SteamID unavailable appid={} method={}",
                           state.appId, state.method.empty() ? "-" : state.method);
            return;
        }

        state.steamIdWritten = Ticket::WriteSteamID(state.appId, steamId);
        LOG_IPCCH_INFO("AuthWindow: persisted active identity appid={} steamid={} ok={} method={}",
                       state.appId, steamId, state.steamIdWritten,
                       state.method.empty() ? "-" : state.method);
    }

} // namespace

namespace AuthWindow {

    void Reset() {
        std::scoped_lock lock(g_lock);
        g_processes.clear();
        g_pipes.clear();
    }

    void OnGamePipe(const PipeWatch::ProcessSnapshot& snapshot, CSteamPipeClient* pipe) {
        if (!pipe || !snapshot.key.IsValid() || !snapshot.likelyGame || !snapshot.luaManaged)
            return;

        const auto probe = ProtectionProbe::ScanOnce(
            snapshot.key.pid, snapshot.key.creation, snapshot.appId, snapshot.imagePath);
        if (!probe.valid)
            return;

        const PipeKey pipeKey = MakePipeKey(pipe);
        if (pipeKey.pid == 0 || pipeKey.pipe == 0)
            return;

        std::scoped_lock lock(g_lock);
        AuthState& state = g_processes[snapshot.key];
        g_pipes[pipeKey] = snapshot.key;

        if (!state.scanned) {
            state.scanned = true;
            state.protectedGame = probe.detected;
            state.appId = snapshot.appId;
            state.method = probe.method;
            LOG_IPCCH_INFO("AuthWindow: scanned pid={} appid={} protected={} method={}",
                           snapshot.key.pid, snapshot.appId, state.protectedGame,
                           state.method.empty() ? "-" : state.method);
        }

        ++state.handshakeCount;
        if (!state.protectedGame) {
            state.window = WindowState::None;
            return;
        }

        if (!state.selectedPipe) {
            state.selectedPipe = pipeKey;
            state.window = WindowState::Open;
            LOG_IPCCH_INFO("AuthWindow: selected pipe pid={} pipe=0x{:08X} appid={} method={}",
                           pipeKey.pid, pipeKey.pipe, state.appId,
                           state.method.empty() ? "-" : state.method);
        }

        if (state.window == WindowState::Open && state.handshakeCount >= kHandshakeWindow) {
            state.window = WindowState::Closed;
            WriteActiveIdentityOnce(state, snapshot.ownedByAccount);
            LOG_IPCCH_INFO("AuthWindow: closed pid={} appid={} handshakes={} state={}",
                           snapshot.key.pid, state.appId, state.handshakeCount, StateName(state.window));
        }
    }

    bool IsSelectedPipe(const CSteamPipeClient* pipe) {
        const PipeKey pipeKey = MakePipeKey(pipe);
        if (pipeKey.pid == 0 || pipeKey.pipe == 0)
            return false;

        std::scoped_lock lock(g_lock);
        const auto pipeIt = g_pipes.find(pipeKey);
        if (pipeIt == g_pipes.end())
            return false;

        const auto procIt = g_processes.find(pipeIt->second);
        if (procIt == g_processes.end())
            return false;

        const AuthState& state = procIt->second;
        return state.protectedGame &&
               state.window == WindowState::Open &&
               state.selectedPipe &&
               *state.selectedPipe == pipeKey;
    }

}
