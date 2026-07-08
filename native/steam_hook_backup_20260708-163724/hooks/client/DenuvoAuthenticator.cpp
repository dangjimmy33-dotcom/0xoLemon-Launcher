// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "hooks/client/DenuvoAuthenticator.h"
#include "runtime/IntegrityScanner.h"
#include "runtime/ProcessInspect.h"
#include "runtime/CredentialStore.h"
#include "runtime/TicketProvider.h"
#include "config/LuaLoader.h"
#include "runtime/Logger.h"
#include "hooks/client/PipeWatch.h"

#include <unordered_map>
#include <optional>
#include <cstdint>
#include <algorithm>

namespace DenuvoAuth {

    namespace {

        constexpr uint32_t kAuthCloseCount = 2;

        enum Stage : uint8_t { Idle, InFlight, Done };
        const char* kStageLabels[] = {"Idle", "InFlight", "Done"};

        uint64_t PackSteam64(uint32_t accountId, uint32_t universe) {
            if (accountId == 0) return 0;
            if (universe == 0) universe = 1;
            return (static_cast<uint64_t>(universe) << 56) |
                   (static_cast<uint64_t>(1)         << 52) |
                   (static_cast<uint64_t>(1)         << 32) |
                   static_cast<uint64_t>(accountId);
        }

        uint32_t UniverseFromName(const std::wstring& name) {
            std::wstring lo(name.size(), L'\0');
            std::transform(name.begin(), name.end(), lo.begin(), ::towlower);
            if (lo == L"public")   return 1;
            if (lo == L"beta")     return 2;
            if (lo == L"internal") return 3;
            if (lo == L"dev")      return 4;
            return 1;
        }

        struct PipeAddr {
            uintptr_t ptr;
            uint32_t  pid;
            bool operator==(const PipeAddr& o) const { return ptr == o.ptr && pid == o.pid; }
        };
        struct PipeAddrHash {
            size_t operator()(const PipeAddr& k) const {
                return std::hash<uintptr_t>()(k.ptr) ^ (std::hash<uint32_t>()(k.pid) << 1);
            }
        };

        using ProcTag = std::pair<uint32_t, uint64_t>;
        struct ProcTagHash {
            size_t operator()(const ProcTag& k) const {
                return std::hash<uint32_t>()(k.first) ^ (std::hash<uint64_t>()(k.second) << 1);
            }
        };

        struct ProcState {
            bool    scanned      = false;
            bool    hasDenuvo    = false;
            Stage   phase        = Stage::Idle;
            uint32_t shakeCount  = 0;
            PipeAddr authPipe{};
            bool    authPipeSet  = false;
            AppId_t authedApp    = 0;
            uint32_t procPid     = 0;
        };

        std::unordered_map<ProcTag, ProcState, ProcTagHash> g_procs;
        std::unordered_map<PipeAddr, ProcTag, PipeAddrHash>   g_pipeLookup;

        ProcState* LookupByPipe(const PipeAddr& addr) {
            auto pi = g_pipeLookup.find(addr);
            if (pi == g_pipeLookup.end()) return nullptr;
            auto ai = g_procs.find(pi->second);
            return ai == g_procs.end() ? nullptr : &ai->second;
        }

        void StampAuthOnFinish(const ProcState& st) {
            if (st.authedApp == 0) return;

            uint64_t sid = 0;
            if (CredentialStore::ReadSteamId(st.authedApp, sid) == CredentialStore::Status::Ok && sid != 0)
                return;

            CredentialStore::ActiveUser au;
            if (CredentialStore::GetActiveUser(au) != CredentialStore::Status::Ok) {
                LOG_AUTH_WARN("DenuvoAuth: finish auth but no active user, app={}", st.authedApp);
                return;
            }

            sid = PackSteam64(au.accountId, UniverseFromName(au.universe));
            if (AppTicket::WriteSteamID(st.authedApp, sid))
                LOG_AUTH_INFO("DenuvoAuth: stamped finish-auth SteamID app={} steam=0x{:X}", st.authedApp, sid);
        }
    }

    void OnHandshake(const CPipeClient* pipe, uint32_t pid, AppId_t appId) {
        if (!pipe || pid == 0) return;

        PipeAddr addr{reinterpret_cast<uintptr_t>(pipe), pid};

        auto tick = ProcessInspect::GetProcessCreationTime(pid);
        if (!tick) return;
        ProcTag tag{pid, *tick};

        g_pipeLookup[addr] = tag;
        ProcState& st = g_procs[tag];

        if (!st.scanned) {
            st.scanned = true;
            auto rep = ProtectionScan::Scan(pid);
            st.hasDenuvo = rep.denuvoDetected;
            st.procPid   = pid;
            if (!st.hasDenuvo &&
                !AppTicket::ReadETicketFromStore(appId).empty()) {
                LOG_AUTH_INFO("DenuvoAuth: scan missed but eticket present; treating as Denuvo app={}", appId);
                st.hasDenuvo = true;
            }
            LOG_AUTH_INFO("DenuvoAuth: scanned pid={} denuvo={}", pid, st.hasDenuvo);
        }

        ++st.shakeCount;

        if (!st.hasDenuvo) {
            st.phase = Stage::Idle;
            return;
        }

        if (!st.authPipeSet) {
            st.authPipe    = addr;
            st.authPipeSet = true;
            st.authedApp   = appId;
            st.phase       = Stage::InFlight;
            LOG_AUTH_INFO("DenuvoAuth: auth pipe picked pid={} appId={}", pid, appId);
        }

        if (st.phase == Stage::InFlight && st.shakeCount >= kAuthCloseCount) {
            st.phase = Stage::Done;
            LOG_AUTH_INFO("DenuvoAuth: auth window closed pid={} appId={}", pid, appId);
            if (LuaLoader::IsOwned(st.authedApp))
                StampAuthOnFinish(st);
        }
    }

    bool IsAuthorizedPipe(const CPipeClient* pipe) {
        if (!pipe) return false;

        uintptr_t needle = reinterpret_cast<uintptr_t>(pipe);

        for (const auto& [addr, tag] : g_pipeLookup) {
            if (addr.ptr != needle) continue;
            auto ai = g_procs.find(tag);
            if (ai == g_procs.end()) return false;
            const auto& st = ai->second;
            if (!st.hasDenuvo) return false;
            if (st.phase != Stage::InFlight) return false;
            if (!st.authPipeSet) return false;
            if (st.authPipe.ptr != needle) return false;

            LOG_AUTH_TRACE("DenuvoAuth: pipe authorized ptr=0x{:X} pid={} app={}",
                            needle, tag.first, st.authedApp);
            return true;
        }
        return false;
    }

    void Init() {
        LOG_AUTH_INFO("DenuvoAuth: ready");
    }

}
