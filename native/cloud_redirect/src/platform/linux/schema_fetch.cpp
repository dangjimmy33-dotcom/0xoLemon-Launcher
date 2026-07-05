#include "schema_fetch.h"
#include "cloud_intercept.h"
#include "metadata_sync.h"
#include "protobuf.h"
#include "cloud_provider_base.h"
#include "vtable_hook.h"
#include "gamesplayed_hook.h"
#include "log.h"

#include <atomic>
#include <cstdio>
#include <cstring>
#include <csetjmp>
#include <csignal>
#include <memory>
#include <mutex>
#include <queue>
#include <thread>
#include <unordered_set>
#include <vector>
#include <sys/stat.h>
#include <unistd.h>

namespace SchemaFetch {

// CProtoBufMsg 32-bit struct layout.
static constexpr size_t MSG_OFF_VTABLE = 0;
static constexpr size_t MSG_OFF_DESC   = 4;   // descriptor / default instance ptr
static constexpr size_t MSG_OFF_EMSG   = 20;  // emsg | 0x80000000
static constexpr size_t MSG_OFF_HDR    = 28;   // header object ptr
static constexpr size_t MSG_OFF_BODY   = 32;   // body object ptr (set by Finalize)
static constexpr size_t MSG_OFF_FLAGS  = 36;
static constexpr size_t MSG_OFF_EXTRA  = 40;

static constexpr uint32_t EMSG_GET_USER_STATS  = 818;
static constexpr uint32_t EMSG_MASK            = 0x7FFFFFFF;

// CMsgProtoBufHeader field numbers (from steammessages_base.proto)
static constexpr uint32_t HDR_STEAMID        = 1;   // fixed64
static constexpr uint32_t HDR_SESSION_ID     = 2;   // int32
static constexpr uint32_t HDR_JOBID_SOURCE   = 10;  // fixed64
static constexpr uint32_t HDR_REALM          = 32;  // uint32
static constexpr uint32_t HDR_TIMEOUT_MS     = 33;  // int32

// CMsgClientGetUserStats body field numbers
static constexpr uint32_t BODY_GAME_ID              = 1;  // fixed64
static constexpr uint32_t BODY_CRC_STATS            = 2;  // varint
static constexpr uint32_t BODY_SCHEMA_LOCAL_VERSION = 3;  // int32 (varint)
static constexpr uint32_t BODY_STEAM_ID_FOR_USER    = 4;  // fixed64

// CMsgClientGetUserStatsResponse (EMsg 819) body field numbers
static constexpr uint32_t RESP_GAME_ID = 1;  // fixed64 (low 24 bits = appid)
static constexpr uint32_t RESP_ERESULT = 2;  // int32 (varint), 1 = OK
static constexpr uint32_t RESP_SCHEMA  = 4;  // bytes (the schema blob)

static constexpr uint32_t EMSG_GET_USER_STATS_RESP = 819;
static constexpr uint32_t PROTO_FLAG               = 0x80000000;

// cmInterface field offsets (32-bit Linux steamclient.so)
static constexpr uint32_t CCM_OFF_SESSION_ID = 0xD8;   // +216
static constexpr uint32_t CCM_OFF_STEAMID    = 0x15C;  // +348
static constexpr uint32_t CCM_OFF_CONNHANDLE = 0x4FC;  // +1276

// Function pointer types.
using CtorFn     = void(*)(void* msg, int emsg, int flags);
using FinalizeFn = void*(*)(void* msg);
using CmSendFn   = uint8_t(*)(void* cmInterface, void* msg);
using CleanupFn  = void(*)(void* msg);

// Resolved entry points.
static uintptr_t       g_base = 0;
static CtorFn          g_ctor = nullptr;
static FinalizeFn      g_finalize = nullptr;
static CmSendFn        g_cmSend = nullptr;
static CleanupFn       g_cleanup = nullptr;
static void*           g_typedVtable = nullptr;   // CProtoBufMsg<CMsgClientGetUserStats> vptr
static void*           g_defaultInstance = nullptr;// CMsgClientGetUserStats default instance
static ParseFromArrayFn g_parseFromArray = nullptr;

// Captured session state.
static std::atomic<uint64_t> g_steamId{0};
static std::atomic<uint32_t> g_sessionId{0};
static std::atomic<uint32_t> g_realm{0};
static std::atomic<bool>     g_sessionCaptured{false};
static std::atomic<uint32_t> g_connHandle{0};
static std::atomic<uint32_t> g_statsConnHandle{0};
// The live CCMInterface pointer (sub_10E6C90's arg_0), captured straight from the
// outbound hook. This is the exact base sub_10E6C90 dereferences (it reads
// [+0x4FC] conn, [+0xD8] session, [+0x15C] steamid). Used to fire schema sends.
static std::atomic<void*>    g_cmInterface{nullptr};

// Schema fetch state.
struct SchemaSendItem { uint32_t appId; uint64_t owner; };
static std::mutex            g_sendMutex;
static std::queue<SchemaSendItem> g_sendQueue;
static std::mutex            g_fetchMutex;
static std::unordered_set<uint32_t> g_fetchAttempted;
static std::atomic<bool>     g_shuttingDown{false};
static std::atomic<bool>     g_sweepScheduled{false};
static std::thread           g_sweepThread;
static thread_local bool     t_draining = false;

// Forward declarations
static void MaybeScheduleSweep();
static void SweepNamespaceSchemas();

// Fallback owners (large-library accounts for apps with no reviews).
static const uint64_t kFallbackOwnerIds[] = {
    76561197978902089ull, 76561198028121353ull, 76561198017975643ull,
    76561198001678750ull, 76561198355953202ull, 76561197993544755ull,
};

// Crash guard (same pattern as AchievementInject).
static sigjmp_buf g_jmp;
static volatile sig_atomic_t g_inCall = 0;
static void CrashHandler(int sig) { if (g_inCall) siglongjmp(g_jmp, sig); raise(sig); }
class CallGuard {
public:
    CallGuard() {
        struct sigaction sa = {};
        sa.sa_handler = CrashHandler;
        sigemptyset(&sa.sa_mask);
        sa.sa_flags = SA_RESETHAND;
        sigaction(SIGSEGV, &sa, &m_segv);
        sigaction(SIGBUS, &sa, &m_bus);
        // Don't trap SIGABRT: it fires async on the CM thread after this guard dies;
        // let init.cpp's CrashDumpHandler own it.
        g_inCall = 1;
    }
    ~CallGuard() {
        g_inCall = 0;
        sigaction(SIGSEGV, &m_segv, nullptr);
        sigaction(SIGBUS, &m_bus, nullptr);
    }
private:
    struct sigaction m_segv = {};
    struct sigaction m_bus = {};
};

// Signature scanning.
static void* ScanSig(uintptr_t base, size_t size,
                     const uint8_t* b, const uint8_t* m, size_t len) {
    if (size < len) return nullptr;
    const uint8_t* s = (const uint8_t*)base;
    const uint8_t* end = s + size - len;
    for (; s <= end; ++s) {
        bool ok = true;
        for (size_t i = 0; i < len; ++i)
            if (m[i] && s[i] != b[i]) { ok = false; break; }
        if (ok) return (void*)s;
    }
    return nullptr;
}

// CProtoBufMsg::ctor: 57 56 53 8B 74 24 10 E8 ?? ?? ?? ?? 81 C3 ?? ?? ?? ??
//   83 EC 08 C7 46 04 00 00 00 00
// push edi; push esi; push ebx; mov esi,[esp+10h]; call PIC; add ebx,??;
// sub esp,8; mov [esi+4],0
static const uint8_t kCtorB[] = {
    0x57,0x56,0x53, 0x8B,0x74,0x24,0x10, 0xE8,0,0,0,0, 0x81,0xC3,0,0,0,0,
    0x83,0xEC,0x08, 0xC7,0x46,0x04,0x00,0x00,0x00,0x00
};
static const uint8_t kCtorM[] = {
    1,1,1, 1,1,1,1, 1,0,0,0,0, 1,1,0,0,0,0,
    1,1,1, 1,1,1,1,1,1,1
};

// CProtoBufMsg::Finalize (sub_2ACE490): 56 53 E8 ?? ?? ?? ?? 81 C3 ?? ?? ?? ??
//   83 EC 04 8B 74 24 10 8B 46 20 85 C0 74 22
// push esi; push ebx; call PIC; add ebx,??; sub esp,4; mov esi,[esp+10h];
// mov eax,[esi+20h]; test eax,eax; jz +0x22. The trailing test/jz is required to
// disambiguate from sub_F82850 (a ScheduledFunction dispatcher with an identical
// prologue through 85 C0, diverging at the jz target). Matches the proven sig in
// live_playtime.cpp (kInitB).
static const uint8_t kFinalizeB[] = {
    0x56,0x53, 0xE8,0,0,0,0, 0x81,0xC3,0,0,0,0, 0x83,0xEC,0x04,
    0x8B,0x74,0x24,0x10, 0x8B,0x46,0x20, 0x85,0xC0,0x74,0x22
};
static const uint8_t kFinalizeM[] = {
    1,1, 1,0,0,0,0, 1,1,0,0,0,0, 1,1,1,
    1,1,1,1, 1,1,1, 1,1,1,1
};

// CM send = CCMInterface::Send (sub_10E6C90). Not sig-scanned: GamesPlayedHook detours
// it first, clobbering the prologue, so we reuse its entry via GetSendFunc().

// RTTI-based vtable/instance resolution.
bool Resolve(uintptr_t base, size_t size, ParseFromArrayFn parseFromArray) {
    g_base = base;
    g_parseFromArray = parseFromArray;

    // Sig-scan for ctor and Finalize.
    g_ctor = (CtorFn)ScanSig(base, size, kCtorB, kCtorM, sizeof(kCtorB));
    g_finalize = (FinalizeFn)ScanSig(base, size, kFinalizeB, kFinalizeM, sizeof(kFinalizeB));

    // Reuse GamesPlayedHook's resolved CCMInterface::Send entry (see note above).
    g_cmSend = (CmSendFn)GamesPlayedHook::GetSendFunc();

    g_typedVtable = VtableHook::FindVtableByRTTIName(
        "12CProtoBufMsgI22CMsgClientGetUserStatsE", base, size);

    // Standalone class name has NO trailing 'E' (that only closes templates); the
    // "...E" form is a false-positive substring of the wrapper's RTTI name.
    void* msgVtable = VtableHook::FindVtableByRTTIName(
        "22CMsgClientGetUserStats", base, size);
    if (msgVtable) {
        g_defaultInstance = VtableHook::FindGlobalWithVtable(msgVtable, base, size);
    }

    // No dtor resolved: each request leaks ~236B (header+body), bounded per session.
    bool ok = g_ctor && g_finalize && g_cmSend && g_typedVtable &&
              g_defaultInstance && g_parseFromArray;
    LOG("[SchemaFetch] Resolve: ctor=%p finalize=%p cmSend=%p vtable=%p desc=%p parse=%p -> %s",
        (void*)g_ctor, (void*)g_finalize, (void*)g_cmSend,
        g_typedVtable, g_defaultInstance, (void*)g_parseFromArray,
        ok ? "OK" : "FAILED");
    return ok;
}

// Session capture.
void CaptureFromOutbound(uint32_t emsg, void* msgObj, void* cmInterface) {
    if (!cmInterface || !msgObj) return;

    // Capture the CCMInterface pointer itself -- this is sub_10E6C90's arg_0.
    g_cmInterface.store(cmInterface, std::memory_order_relaxed);

    // Capture connection handle from cmInterface
    uint32_t conn = *(uint32_t*)((uint8_t*)cmInterface + CCM_OFF_CONNHANDLE);
    if (conn != 0) {
        // Prefer the handle from GetUserStats (818) -- same CM connection
        if (emsg == EMSG_GET_USER_STATS) {
            if (g_statsConnHandle.exchange(conn, std::memory_order_relaxed) != conn)
                LOG("[SchemaFetch] captured conn=%u from Steam's GetUserStats", conn);
        }
        g_connHandle.store(conn, std::memory_order_relaxed);
    }

    // Capture session fields from cmInterface (always valid once logged in)
    if (!g_sessionCaptured.load(std::memory_order_relaxed)) {
        uint64_t sid = *(uint64_t*)((uint8_t*)cmInterface + CCM_OFF_STEAMID);
        uint32_t ses = *(uint32_t*)((uint8_t*)cmInterface + CCM_OFF_SESSION_ID);
        if (sid != 0) {
            g_steamId.store(sid, std::memory_order_relaxed);
            g_sessionId.store(ses, std::memory_order_relaxed);
            // Realm: not directly accessible from cmInterface at a known offset.
            // Default realm for public Steam is 1 (EUniverse_Public).
            g_realm.store(1, std::memory_order_relaxed);
            g_sessionCaptured.store(true, std::memory_order_relaxed);
            LOG("[SchemaFetch] captured session: steamid=0x%llX session=%u",
                (unsigned long long)sid, ses);
        }
    }

    // Schedule the proactive schema sweep once we have session + connection
    if (g_sessionCaptured.load(std::memory_order_relaxed) &&
        g_connHandle.load(std::memory_order_relaxed) != 0) {
        MaybeScheduleSweep();
    }
}

// HTTP owner discovery (uses libcurl via IHttpTransport).
static std::vector<uint64_t> FetchReviewOwnerIds(uint32_t appId) {
    std::vector<uint64_t> ids;
    auto transport = CreateHttpTransport("[SchemaFetch]");
    if (!transport || !transport->Init()) return ids;

    std::string path = "/appreviews/" + std::to_string(appId) +
        "?json=1&filter=recent&language=all&purchase_type=all&num_per_page=20";
    auto resp = transport->Request("GET", "store.steampowered.com", path, {}, {});
    if (resp.status != 200 || resp.body.empty()) return ids;

    // Parse "steamid":"<digits>" from JSON
    constexpr uint64_t kSteamId64Base = 76561197960265728ull;
    size_t pos = 0;
    while ((pos = resp.body.find("\"steamid\"", pos)) != std::string::npos) {
        pos = resp.body.find(':', pos);
        if (pos == std::string::npos) break;
        ++pos;
        while (pos < resp.body.size() && (resp.body[pos] == ' ' || resp.body[pos] == '"')) ++pos;
        size_t start = pos;
        while (pos < resp.body.size() && resp.body[pos] >= '0' && resp.body[pos] <= '9') ++pos;
        if (start == pos) continue;
        uint64_t sid = strtoull(resp.body.c_str() + start, nullptr, 10);
        if (sid >= kSteamId64Base) {
            bool dup = false;
            for (uint64_t existing : ids) if (existing == sid) { dup = true; break; }
            if (!dup) ids.push_back(sid);
        }
    }
    return ids;
}

static bool HasPublicStats(uint32_t appId, uint64_t steamId) {
    auto transport = CreateHttpTransport("[SchemaFetch]");
    if (!transport || !transport->Init()) return false;

    std::string path = "/profiles/" + std::to_string(steamId) +
        "/stats/" + std::to_string(appId) + "/?xml=1";
    auto resp = transport->Request("GET", "steamcommunity.com", path, {}, {});
    return resp.status == 200 &&
           resp.body.find("<playerstats>") != std::string::npos &&
           resp.body.find("<privacyState>public</privacyState>") != std::string::npos;
}

// Send schema request.
static bool SendSchemaRequest(uint32_t appId, uint64_t ownerId,
                              void* cmInterface, uint32_t connHandle) {
    if (!g_ctor || !g_finalize || !g_cmSend || !g_typedVtable ||
        !g_defaultInstance || !g_parseFromArray || !cmInterface)
        return false;

    // CProtoBufMsg<CMsgClientGetUserStats> on the stack (~44B; over-allocate).
    alignas(16) uint8_t msg[128] = {0};

    CallGuard guard;
    int jmpSig = sigsetjmp(g_jmp, 1);
    if (jmpSig != 0) {
        LOG("[SchemaFetch] SendSchemaRequest app=%u caught signal %d -- aborting", appId, jmpSig);
        return false;
    }

    g_ctor(msg, (int)EMSG_GET_USER_STATS, 0);
    *(void**)(msg + MSG_OFF_VTABLE) = g_typedVtable;
    *(void**)(msg + MSG_OFF_DESC) = g_defaultInstance;
    g_finalize(msg);

    void* body = *(void**)(msg + MSG_OFF_BODY);
    void* hdr  = *(void**)(msg + MSG_OFF_HDR);
    if (!body || !hdr) {
        LOG("[SchemaFetch] app=%u: body=%p hdr=%p NULL -> bail", appId, body, hdr);
        return false;
    }

    PB::Writer bodyW;
    bodyW.WriteFixed64(BODY_GAME_ID, (uint64_t)appId);
    bodyW.WriteVarint(BODY_CRC_STATS, 0);
    // schema_local_version = -1: sign-extended 64-bit varint (forces "send latest")
    bodyW.WriteVarint(BODY_SCHEMA_LOCAL_VERSION, (uint64_t)(int64_t)(-1));
    bodyW.WriteFixed64(BODY_STEAM_ID_FOR_USER, ownerId);
    if (!g_parseFromArray(body, bodyW.Data().data(), (int)bodyW.Size())) {
        LOG("[SchemaFetch] body ParseFromArray failed for app=%u", appId);
        return false;
    }

    // jobid_source must be a unique non-negative id (mirrors Windows); the CM
    // routes the 819 reply back by it. -1 means "no source job" -> no reply.
    static std::atomic<uint64_t> s_jobIdCounter{0x5C00000000000001ull};
    uint64_t jobId = s_jobIdCounter.fetch_add(1, std::memory_order_relaxed);
    PB::Writer hdrW;
    hdrW.WriteFixed64(HDR_STEAMID, g_steamId.load(std::memory_order_relaxed));
    hdrW.WriteVarint(HDR_SESSION_ID, (uint64_t)(uint32_t)g_sessionId.load(std::memory_order_relaxed));
    hdrW.WriteFixed64(HDR_JOBID_SOURCE, jobId);
    hdrW.WriteVarint(HDR_REALM, (uint64_t)g_realm.load(std::memory_order_relaxed));
    // timeout_ms = -1 (no deadline). Required: assertion at msgprotobuf.cpp:980.
    hdrW.WriteVarint(HDR_TIMEOUT_MS, (uint64_t)(int64_t)(-1));
    if (!g_parseFromArray(hdr, hdrW.Data().data(), (int)hdrW.Size())) {
        LOG("[SchemaFetch] header ParseFromArray failed for app=%u", appId);
        return false;
    }

    uint8_t sent = g_cmSend(cmInterface, msg);
    LOG("[SchemaFetch] SendSchemaRequest app=%u owner=%llu jobid=0x%llX -> %s",
        appId, (unsigned long long)ownerId,
        (unsigned long long)jobId, sent ? "sent" : "FAILED");
    return sent != 0;
}

// Request + queue logic.
static void RequestSchemaForApp(uint32_t appId) {
    if (!MetadataSync::SchemaFetchEnabled()) return;
    if (appId == 0) return;
    if (g_shuttingDown.load(std::memory_order_acquire)) return;
    if (g_connHandle.load(std::memory_order_relaxed) == 0) return;

    {
        std::lock_guard<std::mutex> lock(g_fetchMutex);
        if (!g_fetchAttempted.insert(appId).second) return;
    }

    std::string steamPath = CloudIntercept::GetSteamPath();
    std::string schemaPath = steamPath + "appcache/stats/UserGameStatsSchema_"
        + std::to_string(appId) + ".bin";

    struct stat st;
    if (stat(schemaPath.c_str(), &st) == 0 && st.st_size > 0) {
        LOG("[SchemaFetch] app %u: schema already on disk (%ld bytes), skipping",
            appId, (long)st.st_size);
        return;
    }
    LOG("[SchemaFetch] app %u: needs fetch", appId);

    // Phase 1: discover owners from reviews + verify public stats
    std::vector<uint64_t> owners = FetchReviewOwnerIds(appId);
    std::vector<uint64_t> verified;
    for (uint64_t sid : owners) {
        if (g_shuttingDown.load(std::memory_order_acquire)) return;
        if (HasPublicStats(appId, sid)) {
            verified.push_back(sid);
            if (verified.size() >= 3) break;
        }
    }

    // Phase 2: fallback owners if review discovery found nothing
    bool usingFallback = verified.empty();
    if (usingFallback) {
        for (uint64_t id : kFallbackOwnerIds)
            verified.push_back(id);
    }
    if (verified.empty()) return;

    // Enqueue sends
    {
        std::lock_guard<std::mutex> lock(g_sendMutex);
        for (uint64_t owner : verified)
            g_sendQueue.push({appId, owner});
    }
    LOG("[SchemaFetch] app %u: queued %zu request(s) via %s",
        appId, verified.size(), usingFallback ? "fallback" : "review-owner discovery");
}

// Drain on net thread.
void DrainOnNetThread() {
    if (!MetadataSync::SchemaFetchEnabled()) return;
    if (t_draining) return;
    if (g_shuttingDown.load(std::memory_order_acquire)) return;

    // Only log/work when the queue actually has something, to avoid spamming
    // every BYieldingSend tick.
    {
        std::lock_guard<std::mutex> lock(g_sendMutex);
        if (g_sendQueue.empty()) return;
    }

    if (!g_sessionCaptured.load(std::memory_order_relaxed)) {
        LOG("[SchemaFetch] Drain: queue non-empty but session not captured, deferring");
        return;
    }

    uint32_t conn = g_statsConnHandle.load(std::memory_order_relaxed);
    if (conn == 0) conn = g_connHandle.load(std::memory_order_relaxed);
    if (conn == 0) {
        LOG("[SchemaFetch] Drain: queue non-empty but conn==0, deferring");
        return;
    }

    void* cmInterface = g_cmInterface.load(std::memory_order_relaxed);
    if (!cmInterface) {
        LOG("[SchemaFetch] Drain: queue non-empty but cmInterface==null, deferring");
        return;
    }

    size_t qsize;
    {
        std::lock_guard<std::mutex> lock(g_sendMutex);
        qsize = g_sendQueue.size();
    }
    LOG("[SchemaFetch] Drain: on net thread, conn=%u, %zu queued", conn, qsize);

    t_draining = true;
    constexpr int kMaxPerTick = 2;
    for (int i = 0; i < kMaxPerTick; ++i) {
        SchemaSendItem item;
        {
            std::lock_guard<std::mutex> lock(g_sendMutex);
            if (g_sendQueue.empty()) break;
            item = g_sendQueue.front();
            g_sendQueue.pop();
        }
        SendSchemaRequest(item.appId, item.owner, cmInterface, conn);
    }
    t_draining = false;
}

// Inbound 819 capture (mirror of Windows TryHandleSchemaResponse).

// Per-user stats template (binary KV cache{ crc=0; PendingChanges=0 }). Steam
// needs UserGameStats_<acctid>_<appid>.bin alongside the schema or stats reading
// fails. Only written when absent so real progress is never clobbered.
static const uint8_t kUserStatsTemplate[38] = {
    0x00,0x63,0x61,0x63,0x68,0x65,0x00,0x02,0x63,0x72,0x63,0x00,0x00,0x00,
    0x00,0x00,0x02,0x50,0x65,0x6e,0x64,0x69,0x6e,0x67,0x43,0x68,0x61,0x6e,
    0x67,0x65,0x73,0x00,0x00,0x00,0x00,0x00,0x08,0x08
};

static bool WriteFileBytes(const std::string& path, const uint8_t* data, size_t len) {
    FILE* f = fopen(path.c_str(), "wb");
    if (!f) return false;
    size_t w = fwrite(data, 1, len, f);
    fclose(f);
    return w == len;
}

bool HandleInbound819(const uint8_t* data, uint32_t len) {
    if (!data || len < 8) return false;

    uint32_t emsgRaw = *(const uint32_t*)data;
    if ((emsgRaw & PROTO_FLAG) == 0) return false;           // not a protobuf msg
    if ((emsgRaw & EMSG_MASK) != EMSG_GET_USER_STATS_RESP) return false;

    uint32_t headerLen = *(const uint32_t*)(data + 4);
    if ((uint64_t)8 + headerLen > len) return false;
    const uint8_t* bodyData = data + 8 + headerLen;
    uint32_t bodyLen = len - 8 - headerLen;
    if (bodyLen == 0) return false;

    auto bodyFields = PB::Parse(bodyData, bodyLen);

    // Correlate by game_id (appid), since the framework assigns its own jobid.
    const PB::Field* gameIdF = PB::FindField(bodyFields, RESP_GAME_ID);
    if (!gameIdF) return false;
    uint32_t appId = (uint32_t)(gameIdF->varintVal & 0xFFFFFF);
    if (appId == 0) return false;

    {
        std::lock_guard<std::mutex> lock(g_fetchMutex);
        if (g_fetchAttempted.find(appId) == g_fetchAttempted.end())
            return false;   // not an app we asked about
    }

    int32_t eresult = 2;
    if (auto* er = PB::FindField(bodyFields, RESP_ERESULT)) eresult = (int32_t)er->varintVal;
    const PB::Field* schemaF = PB::FindField(bodyFields, RESP_SCHEMA);

    bool hasSchema = (eresult == 1 && schemaF &&
                      schemaF->wireType == PB::LengthDelimited && schemaF->dataLen > 0);
    if (!hasSchema) {
        // Owner doesn't own the game or sent no schema; another owner's reply may land it.
        LOG("[SchemaFetch] HandleInbound819: app %u no schema (eresult=%d)", appId, eresult);
        return true;
    }

    std::string steamPath = CloudIntercept::GetSteamPath();
    std::string schemaPath = steamPath + "appcache/stats/UserGameStatsSchema_"
        + std::to_string(appId) + ".bin";

    // Skip if a file already exists with the same size (no new achievements);
    // overwrite if size differs (developer added/removed achievements).
    struct stat st;
    if (stat(schemaPath.c_str(), &st) == 0 && st.st_size > 0) {
        if ((uint32_t)st.st_size == schemaF->dataLen) return true;
        LOG("[SchemaFetch] app %u: schema changed (%ld -> %u bytes), updating",
            appId, (long)st.st_size, schemaF->dataLen);
    }

    if (!WriteFileBytes(schemaPath, schemaF->data, schemaF->dataLen)) {
        LOG("[SchemaFetch] app %u: failed to write %s", appId, schemaPath.c_str());
        return true;
    }
    LOG("[SchemaFetch] app %u: wrote schema (%u bytes) from server response",
        appId, schemaF->dataLen);

    // Per-user stats file -- only create if absent.
    uint32_t acctId = CloudIntercept::GetAccountId();
    if (acctId != 0) {
        std::string statsPath = steamPath + "appcache/stats/UserGameStats_"
            + std::to_string(acctId) + "_" + std::to_string(appId) + ".bin";
        struct stat ss;
        if (stat(statsPath.c_str(), &ss) != 0) {
            if (WriteFileBytes(statsPath, kUserStatsTemplate, sizeof(kUserStatsTemplate)))
                LOG("[SchemaFetch] app %u: wrote per-user stats template (acct %u)", appId, acctId);
        }
    }
    return true;
}

// Proactive sweep.
static void SweepNamespaceSchemas() {
    if (!MetadataSync::SchemaFetchEnabled()) return;
    if (g_connHandle.load(std::memory_order_relaxed) == 0) return;

    auto apps = CloudIntercept::GetNamespaceApps();
    if (apps.empty()) return;

    LOG("[SchemaFetch] Proactive sweep: checking %zu namespace app(s)", apps.size());

    std::string steamPath = CloudIntercept::GetSteamPath();
    std::vector<uint32_t> needed;
    for (uint32_t appId : apps) {
        std::string path = steamPath + "appcache/stats/UserGameStatsSchema_"
            + std::to_string(appId) + ".bin";
        struct stat st;
        if (stat(path.c_str(), &st) != 0 || st.st_size == 0) {
            needed.push_back(appId);
        }
    }
    if (needed.empty()) {
        LOG("[SchemaFetch] All schemas present on disk");
        return;
    }

    LOG("[SchemaFetch] %zu app(s) need schemas, fetching with 4 workers", needed.size());

    constexpr int kWorkers = 4;
    std::atomic<size_t> idx{0};
    std::atomic<int> totalRequested{0};
    std::vector<std::thread> workers;
    for (int w = 0; w < kWorkers; ++w) {
        workers.emplace_back([&needed, &idx, &totalRequested] {
            while (true) {
                if (g_shuttingDown.load(std::memory_order_acquire)) break;
                size_t i = idx.fetch_add(1, std::memory_order_relaxed);
                if (i >= needed.size()) break;
                RequestSchemaForApp(needed[i]);
                totalRequested.fetch_add(1, std::memory_order_relaxed);
            }
        });
    }
    for (auto& t : workers) t.join();
    LOG("[SchemaFetch] Sweep complete: enqueued schemas for %d app(s)",
        totalRequested.load(std::memory_order_relaxed));
}

static void MaybeScheduleSweep() {
    if (!MetadataSync::SchemaFetchEnabled()) return;
    if (g_sweepScheduled.exchange(true)) return;
    g_sweepThread = std::thread([] {
        constexpr int kSettleMs = 15000;
        for (int waited = 0; waited < kSettleMs; waited += 500) {
            if (g_shuttingDown.load(std::memory_order_acquire)) return;
            usleep(500000);
        }
        if (g_shuttingDown.load(std::memory_order_acquire)) return;
        SweepNamespaceSchemas();
    });
}

// Shutdown.
void Shutdown() {
    g_shuttingDown.store(true, std::memory_order_release);
    if (g_sweepThread.joinable())
        g_sweepThread.join();
}

} // namespace SchemaFetch
