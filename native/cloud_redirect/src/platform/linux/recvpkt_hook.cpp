#include "recvpkt_hook.h"
#include "schema_fetch.h"
#include "metadata_sync.h"
#include "log.h"

#include <atomic>
#include <csetjmp>
#include <csignal>
#include <cstring>
#include <initializer_list>
#include <sys/mman.h>
#include <unistd.h>

namespace RecvPktHook {

// EMsg constants (low 31 bits; bit 31 = CMsgBase protobuf flag).
static constexpr uint32_t EMSG_MASK = 0x7FFFFFFF;
static constexpr uint32_t PROTO_FLAG = 0x80000000;
static constexpr uint32_t EMSG_GET_USER_STATS_RESP = 819;

// Raw netpacket layout (resolved from sub_2AC5EC0, the wrapper RecvPkt calls):
//   +4 = data pointer (pubData), +8 = length (cubData). data[0] = emsg | flags.
static constexpr size_t PKT_OFF_DATA = 4;
static constexpr size_t PKT_OFF_LEN  = 8;

static std::atomic<bool> g_installed{false};
static std::atomic<bool> g_shuttingDown{false};
static std::atomic<int>  g_inFlight{0};

// Detour bookkeeping.
static uint8_t* g_funcStart = nullptr;     // resolved CCMInterface::RecvPkt entry
static uint8_t  g_savedBytes[8];           // original prologue bytes
static size_t   g_savedLen = 0;
static uint8_t* g_trampoline = nullptr;    // stolen bytes + jmp back

// RecvPkt uses an EBP frame, so we detour at the entry and steal exactly the
// first 5 bytes (push ebp; mov ebp,esp; push edi; push esi -- all position
// independent, exactly E9-rel32 sized). We must NOT steal into the following PIC
// call (get_pc_thunk + add esi,delta), which computes the GOT base from its own
// return EIP and would break if relocated to the trampoline.
//   55                push ebp
//   89 E5             mov  ebp, esp
//   57                push edi
//   56                push esi
//   E8 ?? ?? ?? ??    call get_pc_thunk          <- left in place
//   81 C6 ?? ?? ?? ?? add  esi, <PIC delta>
//   53                push ebx
//   81 EC CC 04 00 00 sub  esp, 0x4CC
//   8B 45 08          mov  eax, [ebp+8]          ; a1 = thisptr
//   8B 7D 0C          mov  edi, [ebp+0xC]        ; a2 = netpacket
static constexpr size_t STOLEN_LEN = 5;
static const uint8_t kSigBytes[] = {
    0x55, 0x89,0xE5, 0x57, 0x56,
    0xE8,0,0,0,0, 0x81,0xC6,0,0,0,0,
    0x53, 0x81,0xEC,0xCC,0x04,0x00,0x00,
    0x8B,0x45,0x08, 0x8B,0x7D,0x0C
};
static const uint8_t kSigMask[] = {
    1, 1,1, 1, 1,
    1,0,0,0,0, 1,1,0,0,0,0,
    1, 1,1,1,1,1,1,
    1,1,1, 1,1,1
};
static constexpr size_t kSigLen = sizeof(kSigBytes);

// Crash guard: the observer runs on Steam's net thread reading attacker-adjacent
// packet memory. A bad read must never take down the client -- catch and skip.
static thread_local sigjmp_buf t_jmp;
static thread_local volatile sig_atomic_t t_inCall = 0;
static struct sigaction g_oldSegv;
static struct sigaction g_oldBus;
static std::atomic<bool> g_guardInstalled{false};

static void CrashHandler(int sig) {
    if (t_inCall) siglongjmp(t_jmp, sig);
    // Not in our code: restore and re-raise.
    sigaction(SIGSEGV, &g_oldSegv, nullptr);
    sigaction(SIGBUS, &g_oldBus, nullptr);
    raise(sig);
}

// Observer: peek at the inbound packet, capture EMsg 819 schema responses.
// Pure pass-through -- never alters or blocks the packet.
extern "C" void RecvPktHook_OnRecv(void* netPacket) {
    g_inFlight.fetch_add(1, std::memory_order_acquire);

    if (!g_shuttingDown.load(std::memory_order_acquire) && netPacket &&
        MetadataSync::SchemaFetchEnabled()) {

        t_inCall = 1;
        if (sigsetjmp(t_jmp, 1) == 0) {
            uint8_t* raw = (uint8_t*)netPacket;
            const uint8_t* data = *(const uint8_t**)(raw + PKT_OFF_DATA);
            uint32_t len = *(const uint32_t*)(raw + PKT_OFF_LEN);
            if (data && len >= 8) {
                uint32_t emsgRaw = *(const uint32_t*)data;
                uint32_t emsg = emsgRaw & EMSG_MASK;
                if ((emsgRaw & PROTO_FLAG) && emsg == EMSG_GET_USER_STATS_RESP) {
                    SchemaFetch::HandleInbound819(data, len);
                }
            }
        }
        t_inCall = 0;
    }
    g_inFlight.fetch_sub(1, std::memory_order_release);
}

// Runtime 32-bit trampoline. At RecvPkt entry the stack is [esp]=ret, [esp+4]=a1,
// [esp+8]=a2 (the netpacket). We snapshot a2, call the observer, then run the
// stolen prologue bytes and jump back to funcStart+STOLEN_LEN.
//
// At trampoline entry (detour is an E9 jmp, pushes no return addr):
//   [esp]=ret, [esp+4]=a1 (thisptr), [esp+8]=a2 (netpacket).
// pushad subtracts 0x20, so a2 is then at [esp+0x20+8] = [esp+0x28].
//
//   pushad                      ; 60
//   push dword [esp+0x28]       ; FF 74 24 28   (a2: orig [esp+8] + 0x20)
//   mov  eax, <OnRecv>          ; B8 xx xx xx xx
//   call eax                    ; FF D0
//   add  esp, 4                 ; 83 C4 04
//   popad                       ; 61
//   <STOLEN_LEN stolen bytes>   ; original prologue
//   push <resume>               ; 68 xx xx xx xx
//   ret                         ; C3
static bool BuildTrampoline(uint8_t* funcStart) {
    long pageSize = sysconf(_SC_PAGESIZE);
    g_trampoline = (uint8_t*)mmap(nullptr, pageSize, PROT_READ | PROT_WRITE | PROT_EXEC,
                                   MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (g_trampoline == MAP_FAILED) {
        g_trampoline = nullptr;
        LOG("[RecvPkt] mmap trampoline failed");
        return false;
    }

    uint8_t* p = g_trampoline;
    auto emit = [&](std::initializer_list<uint8_t> bytes) {
        for (uint8_t b : bytes) *p++ = b;
    };
    auto emit32 = [&](uint32_t v) {
        *p++ = v & 0xFF; *p++ = (v >> 8) & 0xFF; *p++ = (v >> 16) & 0xFF; *p++ = (v >> 24) & 0xFF;
    };

    emit({0x60});                         // pushad (esp -= 0x20)
    emit({0xFF, 0x74, 0x24, 0x28});       // push [esp+0x28]  -> a2 (orig [esp+8] + 0x20)
    emit({0xB8}); emit32((uint32_t)(uintptr_t)&RecvPktHook_OnRecv); // mov eax, OnRecv
    emit({0xFF, 0xD0});                   // call eax
    emit({0x83, 0xC4, 0x04});             // add esp, 4
    emit({0x61});                         // popad

    memcpy(p, funcStart, STOLEN_LEN);     // stolen prologue
    p += STOLEN_LEN;

    uintptr_t resume = (uintptr_t)funcStart + STOLEN_LEN;
    emit({0x68}); emit32((uint32_t)resume); // push resume
    emit({0xC3});                           // ret
    return true;
}

static bool FindBySignature(uintptr_t base, size_t size, uint8_t*& outFuncStart) {
    if (size < kSigLen) return false;
    const uint8_t* start = (const uint8_t*)base;
    const uint8_t* end = start + size - kSigLen;
    for (const uint8_t* s = start; s <= end; ++s) {
        bool match = true;
        for (size_t i = 0; i < kSigLen; ++i) {
            if (kSigMask[i] && s[i] != kSigBytes[i]) { match = false; break; }
        }
        if (match) { outFuncStart = const_cast<uint8_t*>(s); return true; }
    }
    return false;
}

static bool MakeWritable(void* addr, size_t len) {
    long pageSize = sysconf(_SC_PAGESIZE);
    uintptr_t page = (uintptr_t)addr & ~(uintptr_t)(pageSize - 1);
    uintptr_t endAddr = (uintptr_t)addr + len;
    size_t pageLen = ((endAddr - page) + (pageSize - 1)) & ~(size_t)(pageSize - 1);
    return mprotect((void*)page, pageLen, PROT_READ | PROT_WRITE | PROT_EXEC) == 0;
}

bool Install(uintptr_t steamclientBase, size_t steamclientSize) {
    bool expected = false;
    if (!g_installed.compare_exchange_strong(expected, true)) return true;

    uint8_t* funcStart = nullptr;
    if (!FindBySignature(steamclientBase, steamclientSize, funcStart)) {
        LOG("[RecvPkt] CCMInterface::RecvPkt signature not found -- inbound capture disabled");
        g_installed.store(false);
        return false;
    }
    LOG("[RecvPkt] CCMInterface::RecvPkt found at %p (sc+0x%zx)",
        funcStart, (size_t)((uintptr_t)funcStart - steamclientBase));

    g_funcStart = funcStart;

    if (!BuildTrampoline(g_funcStart)) {
        g_installed.store(false);
        return false;
    }

    // Install the SIGSEGV/SIGBUS guard once (process-wide, restored on Remove).
    bool guardExpected = false;
    if (g_guardInstalled.compare_exchange_strong(guardExpected, true)) {
        struct sigaction sa = {};
        sa.sa_handler = CrashHandler;
        sigemptyset(&sa.sa_mask);
        sa.sa_flags = 0;
        sigaction(SIGSEGV, &sa, &g_oldSegv);
        sigaction(SIGBUS, &sa, &g_oldBus);
    }

    g_savedLen = STOLEN_LEN;
    memcpy(g_savedBytes, g_funcStart, g_savedLen);

    if (!MakeWritable(g_funcStart, g_savedLen)) {
        LOG("[RecvPkt] mprotect RWX failed at entry");
        g_installed.store(false);
        return false;
    }

    // E9 rel32 jmp to trampoline (exactly 5 bytes = STOLEN_LEN).
    int32_t rel = (int32_t)((uintptr_t)g_trampoline - ((uintptr_t)g_funcStart + 5));
    g_funcStart[0] = 0xE9;
    memcpy(g_funcStart + 1, &rel, 4);

    __builtin___clear_cache((char*)g_funcStart, (char*)g_funcStart + g_savedLen);
    LOG("[RecvPkt] Inline detour installed at %p -> trampoline %p", g_funcStart, g_trampoline);
    return true;
}

void Remove() {
    if (!g_installed.load(std::memory_order_acquire)) return;
    g_shuttingDown.store(true, std::memory_order_release);

    if (g_funcStart && MakeWritable(g_funcStart, g_savedLen)) {
        memcpy(g_funcStart, g_savedBytes, g_savedLen);
        __builtin___clear_cache((char*)g_funcStart, (char*)g_funcStart + g_savedLen);
    }
    for (int i = 0; i < 300 && g_inFlight.load(std::memory_order_acquire) > 0; ++i)
        usleep(10000); // up to 3s

    if (g_guardInstalled.exchange(false)) {
        sigaction(SIGSEGV, &g_oldSegv, nullptr);
        sigaction(SIGBUS, &g_oldBus, nullptr);
    }
    g_installed.store(false, std::memory_order_release);
}

} // namespace RecvPktHook
