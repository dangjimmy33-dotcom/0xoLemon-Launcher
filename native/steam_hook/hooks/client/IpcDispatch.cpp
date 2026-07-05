// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

// IpcDispatch registers runtime ticket-spoofing IPC handlers through the
// existing IPCBus system. Instead of installing its own IPCProcessMessage
// hook (which would collide with IPCBus's existing hook), it converts the
// pre/post handler model into IpcHandlerEntry slots that IPCBus dispatches
// from its own hook. The side benefit is that IPCBus's pipe-scoping and
// logging already covers every dispatch path so IpcDispatch handlers don't
// need to duplicate that bookkeeping.

#include "hooks/client/IpcDispatch.h"
#include "hooks/client/IPCBus.h"
#include "hooks/client/IpcMethodLoader.h"
#include "runtime/Logger.h"
#include <algorithm>
#include <cstring>
#include <deque>
#include <vector>

namespace IpcDispatch {

    namespace {

        struct RegisteredEntry {
            std::string ifaceName;
            std::string methodName;
            PreFn pre;
            PostFn post;
        };

        std::vector<RegisteredEntry> g_pending;

        // Persist dynamically-constructed handler names so busEntry.name
        // never points to a destroyed temporary.
        std::deque<std::string> g_handlerNames;

        static void IpcBusTrampoline(CSteamPipeClient* pipe, CUtlBuffer* pRead, CUtlBuffer* pWrite) {
            (void)pipe; (void)pRead; (void)pWrite;
        }
    }

    void Register(std::string_view ifaceName, std::string_view methodName, PreFn pre, PostFn post) {
        // Look up the func hash from IpcLoader metadata. If not found,
        // the handler won't match any IPC dispatch in IPCBus.
        const auto* meta = IpcLoader::Find(ifaceName, methodName);
        if (!meta) {
            LOG_WARN("IpcDispatch: no IPC metadata for {}::{} - handler disabled", ifaceName, methodName);
            return;
        }

        LOG_IPC_DEBUG("IpcDispatch: registering {}::{} hash=0x{:08X}", ifaceName, methodName, meta->funcHash);
        g_pending.push_back({std::string(ifaceName), std::string(methodName), pre, post});
    }

    void Install() {
        if (g_pending.empty()) {
            LOG_IPC_INFO("IpcDispatch: no handlers registered, skipping");
            return;
        }

        size_t registered = 0;
        for (const auto& entry : g_pending) {
            const auto* meta = IpcLoader::Find(entry.ifaceName, entry.methodName);
            if (!meta) continue;

            // Register through IPCBus so existing piping/logging works
            IPCBus::IpcHandlerEntry busEntry{};
            g_handlerNames.push_back(entry.ifaceName + "::" + entry.methodName);
            busEntry.name = g_handlerNames.back().c_str();

            // Map EIPCInterface name to enum via static lookup table
            static constexpr std::pair<const char*, EIPCInterface> kIfaceMap[] = {
                {"IClientUser",       EIPCInterface::IClientUser},
                {"IClientUserStats",  EIPCInterface::IClientUserStats},
                {"IClientUtils",      EIPCInterface::IClientUtils},
                {"IClientAppManager", EIPCInterface::IClientAppManager},
            };
            auto it = std::find_if(std::begin(kIfaceMap), std::end(kIfaceMap),
                [&](auto& p) { return std::strcmp(p.first, entry.ifaceName.c_str()) == 0; });
            if (it == std::end(kIfaceMap)) {
                LOG_WARN("IpcDispatch: unknown interface {} - skipping", entry.ifaceName);
                continue;
            }
            busEntry.interfaceID = it->second;

            busEntry.funcHash = meta->funcHash;
            busEntry.handler = entry.post; // post-handler runs after original

            IPCBus::RegisterHandlers(&busEntry, 1);
            ++registered;
            LOG_IPC_DEBUG("IpcDispatch: installed handler for {}::{}", entry.ifaceName, entry.methodName);
        }

        LOG_IPC_INFO("IpcDispatch: installed {} handler(s) via IPCBus", registered);
        g_pending.clear();
    }

    void Uninstall() {
        g_pending.clear();
    }

}
