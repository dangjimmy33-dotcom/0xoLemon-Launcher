/* Copyright (C) 2019 Mr Goldberg
   This file is part of the Goldberg Emulator

   The Goldberg Emulator is free software; you can redistribute it and/or
   modify it under the terms of the GNU Lesser General Public
   License as published by the Free Software Foundation; either
   version 3 of the License, or (at your option) any later version.

   The Goldberg Emulator is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
   Lesser General Public License for more details.

   You should have received a copy of the GNU Lesser General Public
   License along with the Goldberg Emulator; if not, see
   <http://www.gnu.org/licenses/>.  */

#include "dll/base.h"
#include "dll/settings_parser.h"

#ifndef EMU_RELEASE_BUILD
#include "dbg_log/dbg_log.hpp"
#endif


std::recursive_mutex global_mutex{};
// some arbitrary counter/time for reference
extern const std::chrono::time_point<std::chrono::high_resolution_clock> startup_counter = std::chrono::high_resolution_clock::now();
extern const std::chrono::time_point<std::chrono::system_clock> startup_time = std::chrono::system_clock::now();

#ifndef EMU_RELEASE_BUILD
dbg_log dbg_logger(get_full_program_path() + "STEAM_LOG_" + std::to_string(common_helpers::rand_number(UINT32_MAX)) + ".log");
#endif


#ifdef __WINDOWS__

void randombytes(char *buf, size_t size)
{
    // NT_SUCCESS is: return value >= 0, including Ntdef.h causes so many errors
    while (BCryptGenRandom(NULL, (PUCHAR) buf, (ULONG) size, BCRYPT_USE_SYSTEM_PREFERRED_RNG) < 0) {
        PRINT_DEBUG("ERROR");
        Sleep(100);
    }
    
}

std::string get_env_variable(const std::string &name)
{
    wchar_t env_variable[1024]{};
    DWORD ret = GetEnvironmentVariableW(utf8_decode(name).c_str(), env_variable, _countof(env_variable));
    if (ret <= 0 || !env_variable[0]) {
        return std::string();
    }

    env_variable[ret] = 0;
    return utf8_encode(env_variable);
}

bool set_env_variable(const std::string &name, const std::string &value)
{
    return SetEnvironmentVariableW(utf8_decode(name).c_str(), utf8_decode(value).c_str());
}

#else

static int fd = -1;

void randombytes(char *buf, size_t size)
{
  int i{};

  if (fd == -1) {
    for (;;) {
      fd = open("/dev/urandom",O_RDONLY);
      if (fd != -1) break;
      sleep(1);
    }
  }

  while (size > 0) {
    if (size < 1048576) i = size; else i = 1048576;

    i = read(fd,buf,i);
    if (i < 1) {
      sleep(1);
      continue;
    }

    buf += i;
    size -= i;
  }
}

std::string get_env_variable(const std::string &name)
{
    char *env = getenv(name.c_str());
    if (!env) {
        return std::string();
    }

    return std::string(env);
}

bool set_env_variable(const std::string &name, const std::string &value)
{
    return setenv(name.c_str(), value.c_str(), 1) == 0;
}

#endif


unsigned generate_account_id()
{
    int a = 0;
    randombytes((char *)&a, sizeof(a));
    a = abs(a);
    if (!a) ++a;
    return a;
}

CSteamID generate_steam_anon_user()
{
    return CSteamID(generate_account_id(), k_unSteamUserDefaultInstance, k_EUniversePublic, k_EAccountTypeAnonUser);
}

SteamAPICall_t generate_steam_api_call_id() {
    static SteamAPICall_t a;
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    
    randombytes((char *)&a, sizeof(a));
    ++a;
    if (a == 0) ++a;
    return a;
}

CSteamID generate_steam_id_user()
{
    return CSteamID(generate_account_id(), k_unSteamUserDefaultInstance, k_EUniversePublic, k_EAccountTypeIndividual);
}

CSteamID generate_steam_id_server()
{
    return CSteamID(generate_account_id(), k_unSteamUserDefaultInstance, k_EUniversePublic, k_EAccountTypeGameServer);
}

CSteamID generate_steam_id_anonserver()
{
    return CSteamID(generate_account_id(), k_unSteamUserDefaultInstance, k_EUniversePublic, k_EAccountTypeAnonGameServer);
}

CSteamID generate_steam_id_lobby()
{
    return CSteamID(generate_account_id(), k_EChatInstanceFlagLobby | k_EChatInstanceFlagMMSLobby, k_EUniversePublic, k_EAccountTypeChat);
}

bool check_timedout(std::chrono::high_resolution_clock::time_point old, double timeout, std::chrono::high_resolution_clock::time_point now) // in seconds
{
    if (timeout == 0.0) return true;

    if (std::chrono::duration_cast<std::chrono::duration<double>>(now - old).count() > timeout) {
        return true;
    }

    return false;
}

std::string get_full_exe_path()
{
    // https://github.com/gpakosz/whereami/blob/master/src/whereami.c
    // https://stackoverflow.com/q/1023306

    static std::string exe_path{};
    static std::recursive_mutex mtx{};

    if (!exe_path.empty()) {
        return exe_path;
    }

    std::lock_guard lock(mtx);
    // check again in case we didn't win this thread arbitration
    if (!exe_path.empty()) {
        return exe_path;
    }

#if defined(__WINDOWS__)
    static wchar_t path[8192]{};
    auto ret = ::GetModuleFileNameW(nullptr, path, _countof(path));
    if (ret >= _countof(path) || 0 == ret) {
        path[0] = '.';
        path[1] = 0;
    }
    exe_path = canonical_path(utf8_encode(path));
#else
    // https://man7.org/linux/man-pages/man5/proc.5.html
    // https://linux.die.net/man/5/proc
    // https://man7.org/linux/man-pages/man2/readlink.2.html
    // https://linux.die.net/man/3/readlink
    static char path[8192]{};
    auto read = ::readlink("/proc/self/exe", path, sizeof(path) - 1);
    if (-1 == read) {
        path[0] = '.';
        read = 1;
    }
    path[read] = 0;
    exe_path = canonical_path(path);
#endif // __WINDOWS__

    return exe_path;
}

std::string get_exe_dirname()
{
    std::string env_exe_dir = get_env_variable("GseExeDir");
    if (!env_exe_dir.empty()) {
        if (env_exe_dir.back() != PATH_SEPARATOR[0]) {
            env_exe_dir = env_exe_dir.append(PATH_SEPARATOR);
        }

        return env_exe_dir;
    }

    std::string full_exe_path = get_full_exe_path();
    return full_exe_path.substr(0, full_exe_path.rfind(PATH_SEPARATOR)).append(PATH_SEPARATOR);

}

#ifdef __LINUX__
std::string get_lib_path()
{
    Dl_info info;
    if (dladdr((void*)get_lib_path, &info))
    {
        return std::string(info.dli_fname);
    }
    return ".";
}
#endif

std::string get_full_lib_path()
{
    std::string program_path;
#if defined(__WINDOWS__)
    wchar_t   DllPath[2048] = {0};
    GetModuleFileNameW((HINSTANCE)&__ImageBase, DllPath, _countof(DllPath));
    program_path = utf8_encode(DllPath);
#else
    program_path = get_lib_path();
#endif
    return program_path;
}

std::string get_full_program_path()
{
    std::string env_program_path = get_env_variable("GseAppPath");
    if (env_program_path.length()) {
        if (env_program_path.back() != PATH_SEPARATOR[0]) {
            env_program_path = env_program_path.append(PATH_SEPARATOR);
        }

        return env_program_path;
    }

    std::string program_path{};
    program_path = get_full_lib_path();
    return program_path.substr(0, program_path.rfind(PATH_SEPARATOR)).append(PATH_SEPARATOR);
}

std::string get_current_path()
{
    std::string path;
#if defined(STEAM_WIN32)
    char *buffer = _getcwd( NULL, 0 );
#else
    char *buffer = get_current_dir_name();
#endif
    if (buffer) {
        path = buffer;
        path.append(PATH_SEPARATOR);
        free(buffer);
    }

    return path;
}

std::string canonical_path(const std::string &path)
{
    std::string output;
#if defined(STEAM_WIN32)
    wchar_t *buffer = _wfullpath(NULL, utf8_decode(path).c_str(), 0);
    if (buffer) {
        output = utf8_encode(buffer);
        free(buffer);
    }
#else
    char *buffer = canonicalize_file_name(path.c_str());
    if (buffer) {
        output = buffer;
        free(buffer);
    }
#endif

    return output;
}

bool file_exists_(const std::string &full_path)
{
#if defined(STEAM_WIN32)
    struct _stat buffer{};
    if (_wstat(utf8_decode(full_path).c_str(), &buffer) != 0)
        return false;

    if ( buffer.st_mode & S_IFDIR)
        return false;
#else
    struct stat buffer{};
    if (stat(full_path.c_str(), &buffer) != 0)
        return false;

    if (S_ISDIR(buffer.st_mode))
        return false;
#endif

    return true;
}

unsigned int file_size_(const std::string &full_path)
{
#if defined(STEAM_WIN32)
    struct _stat buffer{};
    if (_wstat(utf8_decode(full_path).c_str(), &buffer) != 0) return 0;
#else
    struct stat buffer{};
    if (stat (full_path.c_str(), &buffer) != 0) return 0;
#endif
    return buffer.st_size;
}


#ifdef EMU_EXPERIMENTAL_BUILD

static std::vector<void*> loaded_libs{};

static void load_dlls()
{
    constexpr static const char LIB_EXTENSION[] =
#ifdef __WINDOWS__
        ".dll"
#else
        ".so"
#endif
        ;

    std::string path(Local_Storage::get_game_settings_path() + "load_dlls" + PATH_SEPARATOR);

    std::vector<std::string> paths(Local_Storage::get_filenames_path(path));
    for (auto & p: paths) {
        std::string full_path(path + p);
        if (!common_helpers::ends_with_i(full_path, LIB_EXTENSION)) continue;

        PRINT_DEBUG("loading '%s'", full_path.c_str());
        auto lib_handle =
#ifdef __WINDOWS__
            LoadLibraryW(utf8_decode(full_path).c_str());
#else
            dlopen(full_path.c_str(), RTLD_NOW | RTLD_LOCAL);
#endif

        if (lib_handle != nullptr) {
            loaded_libs.push_back(reinterpret_cast<void *>(lib_handle));
            PRINT_DEBUG(" LOADED");
        } else {
#ifdef __WINDOWS__
            PRINT_DEBUG(" FAILED, error code 0x%X", GetLastError());
#else
            PRINT_DEBUG(" FAILED, error string '%s'", dlerror());
#endif
        }
    }
}

static void unload_dlls()
{
    for (auto lib_handle : loaded_libs) {
#ifdef __WINDOWS__
        FreeLibrary(reinterpret_cast<HMODULE>(lib_handle));
#else
        dlclose(lib_handle);
#endif
    }
}

#ifdef __WINDOWS__

#include <TlHelp32.h>

struct ips_test {
    uint32_t ip_from;
    uint32_t ip_to;
};

static std::vector<struct ips_test> whitelist_ips;

void set_whitelist_ips(uint32_t *from, uint32_t *to, unsigned num_ips)
{
    whitelist_ips.clear();
    for (unsigned i = 0; i < num_ips; ++i) {
        struct ips_test ip_a;
        PRINT_DEBUG("from: %hhu.%hhu.%hhu.%hhu", ((unsigned char *)&from[i])[0], ((unsigned char *)&from[i])[1], ((unsigned char *)&from[i])[2], ((unsigned char *)&from[i])[3]);
        PRINT_DEBUG("to: %hhu.%hhu.%hhu.%hhu", ((unsigned char *)&to[i])[0], ((unsigned char *)&to[i])[1], ((unsigned char *)&to[i])[2], ((unsigned char *)&to[i])[3]);
        ip_a.ip_from = ntohl(from[i]);
        ip_a.ip_to = ntohl(to[i]);
        if (ip_a.ip_to < ip_a.ip_from) continue;
        if ((ip_a.ip_to - ip_a.ip_from) > (1 << 25)) continue;
        PRINT_DEBUG("added ip to whitelist");
        whitelist_ips.push_back(ip_a);
    }
}

static bool is_whitelist_ip(unsigned char *ip)
{
    uint32_t ip_temp = 0;
    memcpy(&ip_temp, ip, sizeof(ip_temp));
    ip_temp = ntohl(ip_temp);

    for (auto &i : whitelist_ips) {
        if (i.ip_from <= ip_temp && ip_temp <= i.ip_to) {
            PRINT_DEBUG("IP IS WHITELISTED %hhu.%hhu.%hhu.%hhu", ip[0], ip[1], ip[2], ip[3]);
            return true;
        }
    }

    return false;
}

static bool is_lan_ipv4(unsigned char *ip)
{
    PRINT_DEBUG("CHECK LAN IP %hhu.%hhu.%hhu.%hhu", ip[0], ip[1], ip[2], ip[3]);
    if (is_whitelist_ip(ip)) return true;
    if (ip[0] == 127) return true;
    if (ip[0] == 10) return true;
    if (ip[0] == 192 && ip[1] == 168) return true;
    if (ip[0] == 169 && ip[1] == 254 && ip[2] != 0) return true;
    if (ip[0] == 172 && ip[1] >= 16 && ip[1] <= 31) return true;
    if ((ip[0] == 100) && ((ip[1] & 0xC0) == 0x40)) return true;
    if (ip[0] == 239) return true; //multicast
    if (ip[0] == 0) return true; //Current network
    if (ip[0] == 192 && (ip[1] == 18 || ip[1] == 19)) return true; //Used for benchmark testing of inter-network communications between two separate subnets.
    if (ip[0] >= 224) return true; //ip multicast (224 - 239) future use (240.0.0.0 - 255.255.255.254) broadcast (255.255.255.255)
    return false;
}

static bool is_lan_ip(const sockaddr *addr, int namelen)
{
    if (!namelen) return false;

    if (addr->sa_family == AF_INET) {
        struct sockaddr_in *addr_in = (struct sockaddr_in *)addr;
        unsigned char ip[4];
        memcpy(ip, &addr_in->sin_addr, sizeof(ip));
        if (is_lan_ipv4(ip)) return true;
    } else if (addr->sa_family == AF_INET6) {
        struct sockaddr_in6 *addr_in6 = (struct sockaddr_in6 *)addr;
        unsigned char ip[16];
        unsigned char zeroes[16] = {};
        memcpy(ip, &addr_in6->sin6_addr, sizeof(ip));
        PRINT_DEBUG("CHECK LAN IP6 %hhu.%hhu.%hhu.%hhu.%hhu.%hhu.%hhu.%hhu.%hhu.%hhu.%hhu.%hhu.%hhu.%hhu.%hhu.%hhu", ip[0], ip[1], ip[2], ip[3], ip[4], ip[5], ip[6], ip[7], ip[8], ip[9], ip[10], ip[11], ip[12], ip[13], ip[14], ip[15]);
        if (((ip[0] == 0xFF) && (ip[1] < 3) && (ip[15] == 1)) ||
        ((ip[0] == 0xFE) && ((ip[1] & 0xC0) == 0x80))) return true;
        if (memcmp(zeroes, ip, sizeof(ip)) == 0) return true;
        if (memcmp(zeroes, ip, sizeof(ip) - 1) == 0 && ip[15] == 1) return true;
        if (ip[0] == 0xff) return true; //multicast
        if (ip[0] == 0xfc) return true; //unique local
        if (ip[0] == 0xfd) return true; //unique local

        unsigned char ipv4_mapped[12] = {};
        ipv4_mapped[10] = 0xFF;
        ipv4_mapped[11] = 0xFF;
        if (memcmp(ipv4_mapped, ip, sizeof(ipv4_mapped)) == 0) {
            if (is_lan_ipv4(ip + 12)) return true;
        }
    }

    PRINT_DEBUG("NOT LAN IP");
    return false;
}

int ( WINAPI *Real_SendTo )( SOCKET s, const char *buf, int len, int flags, const sockaddr *to, int tolen) = sendto;
int ( WINAPI *Real_Connect )( SOCKET s, const sockaddr *addr, int namelen ) = connect;
int ( WINAPI *Real_WSAConnect )( SOCKET s, const sockaddr *addr, int namelen, LPWSABUF lpCallerData, LPWSABUF lpCalleeData, LPQOS lpSQOS, LPQOS lpGQOS) = WSAConnect;

static int WINAPI Mine_SendTo( SOCKET s, const char *buf, int len, int flags, const sockaddr *to, int tolen)
{
    PRINT_DEBUG_ENTRY();
    if (is_lan_ip(to, tolen)) {
        return Real_SendTo( s, buf, len, flags, to, tolen );
    } else {
        return len;
    }
}

static int WINAPI Mine_Connect( SOCKET s, const sockaddr *addr, int namelen )
{
    PRINT_DEBUG_ENTRY();
    if (is_lan_ip(addr, namelen)) {
        return Real_Connect(s, addr, namelen);
    } else {
        WSASetLastError(WSAECONNREFUSED);
        return SOCKET_ERROR;
    }
}

static int WINAPI Mine_WSAConnect( SOCKET s, const sockaddr *addr, int namelen, LPWSABUF lpCallerData, LPWSABUF lpCalleeData, LPQOS lpSQOS, LPQOS lpGQOS)
{
    PRINT_DEBUG_ENTRY();
    if (is_lan_ip(addr, namelen)) {
        return Real_WSAConnect(s, addr, namelen, lpCallerData, lpCalleeData, lpSQOS, lpGQOS);
    } else {
        WSASetLastError(WSAECONNREFUSED);
        return SOCKET_ERROR;
    }
}

inline bool file_exists (const std::string& name)
{
  struct stat buffer;   
  return (stat (name.c_str(), &buffer) == 0); 
}

#ifdef DETOURS_64BIT
    #define DLL_NAME "steam_api64.dll"
#else
    #define DLL_NAME "steam_api.dll"
#endif

HMODULE (WINAPI *Real_GetModuleHandleA)(LPCSTR lpModuleName) = GetModuleHandleA;
HMODULE WINAPI Mine_GetModuleHandleA(LPCSTR lpModuleName)
{
    PRINT_DEBUG("%s", lpModuleName);
    if (!lpModuleName) return Real_GetModuleHandleA(lpModuleName);
    std::string in(lpModuleName);
    if (in == std::string(DLL_NAME)) {
        in = std::string("crack") + in;
    }

    return Real_GetModuleHandleA(in.c_str());
}

static void redirect_crackdll()
{
    DetourTransactionBegin();
    DetourUpdateThread( GetCurrentThread() );
    DetourAttach( reinterpret_cast<PVOID*>(&Real_GetModuleHandleA), reinterpret_cast<PVOID>(Mine_GetModuleHandleA) );
    DetourTransactionCommit();
}

static void unredirect_crackdll()
{
    DetourTransactionBegin();
    DetourUpdateThread( GetCurrentThread() );
    DetourDetach( reinterpret_cast<PVOID*>(&Real_GetModuleHandleA), reinterpret_cast<PVOID>(Mine_GetModuleHandleA) );
    DetourTransactionCommit();
}

HMODULE crack_dll_handle{};
static void load_crack_dll()
{
    std::string path(get_full_program_path() + "crack" + DLL_NAME);
    PRINT_DEBUG("searching for crack file '%s'", path.c_str());
    if (file_exists(path)) {
        redirect_crackdll();
        crack_dll_handle = LoadLibraryW(utf8_decode(path).c_str());
        unredirect_crackdll();
        PRINT_DEBUG("Loaded crack file");
    }
}

#include "dll/local_storage.h"

//For some reason when this function is optimized it breaks the shogun 2 prophet (reloaded) crack.
#pragma optimize( "", off )
bool crack_SteamAPI_RestartAppIfNecessary(uint32 unOwnAppID)
{
    if (crack_dll_handle) {
        bool (__stdcall* restart_app)(uint32) = (bool (__stdcall *)(uint32))GetProcAddress(crack_dll_handle, "SteamAPI_RestartAppIfNecessary");
        if (restart_app) {
            PRINT_DEBUG("Calling crack SteamAPI_RestartAppIfNecessary");
            redirect_crackdll();
            bool ret = restart_app(unOwnAppID);
            unredirect_crackdll();
            return ret;
        }
    }

    return false;
}
#pragma optimize( "", on )

bool crack_SteamAPI_Init()
{
    if (crack_dll_handle) {
        bool (__stdcall* init_app)() = (bool (__stdcall *)())GetProcAddress(crack_dll_handle, "SteamAPI_Init");
        if (init_app) {
            PRINT_DEBUG("Calling crack SteamAPI_Init");
            redirect_crackdll();
            bool ret = init_app();
            unredirect_crackdll();
            return ret;
        }
    }

    return false;
}

HINTERNET (WINAPI *Real_WinHttpConnect)(
  IN HINTERNET     hSession,
  IN LPCWSTR       pswzServerName,
  IN INTERNET_PORT nServerPort,
  IN DWORD         dwReserved
);

HINTERNET WINAPI Mine_WinHttpConnect(
  IN HINTERNET     hSession,
  IN LPCWSTR       pswzServerName,
  IN INTERNET_PORT nServerPort,
  IN DWORD         dwReserved
) {
    PRINT_DEBUG("%ls %u", pswzServerName, nServerPort);
    struct sockaddr_in ip4;
    struct sockaddr_in6 ip6;
    ip4.sin_family = AF_INET;
    ip6.sin6_family = AF_INET6;

    if ((InetPtonW(AF_INET, pswzServerName, &(ip4.sin_addr)) && is_lan_ip((sockaddr *)&ip4, sizeof(ip4))) || (InetPtonW(AF_INET6, pswzServerName, &(ip6.sin6_addr)) && is_lan_ip((sockaddr *)&ip6, sizeof(ip6)))) {
        return Real_WinHttpConnect(hSession, pswzServerName, nServerPort, dwReserved);
    } else {
        return Real_WinHttpConnect(hSession, L"127.1.33.7", nServerPort, dwReserved);
    }
}

HINTERNET (WINAPI *Real_WinHttpOpenRequest)(
  IN HINTERNET hConnect,
  IN LPCWSTR   pwszVerb,
  IN LPCWSTR   pwszObjectName,
  IN LPCWSTR   pwszVersion,
  IN LPCWSTR   pwszReferrer,
  IN LPCWSTR   *ppwszAcceptTypes,
  IN DWORD     dwFlags
);

HINTERNET WINAPI Mine_WinHttpOpenRequest(
  IN HINTERNET hConnect,
  IN LPCWSTR   pwszVerb,
  IN LPCWSTR   pwszObjectName,
  IN LPCWSTR   pwszVersion,
  IN LPCWSTR   pwszReferrer,
  IN LPCWSTR   *ppwszAcceptTypes,
  IN DWORD     dwFlags
) {
    PRINT_DEBUG("%ls %ls %ls %ls %i", pwszVerb, pwszObjectName, pwszVersion, pwszReferrer, dwFlags);
    if (dwFlags & WINHTTP_FLAG_SECURE) {
        dwFlags ^= WINHTTP_FLAG_SECURE;
    }

    return Real_WinHttpOpenRequest(hConnect, pwszVerb, pwszObjectName, pwszVersion, pwszReferrer, ppwszAcceptTypes, dwFlags);
}


static bool network_functions_attached = false;

// read command line from a remote process via its PEB
static std::wstring read_process_cmdline(HANDLE hProc, DWORD pid)
{
    if (pid == GetCurrentProcessId()) {
        return GetCommandLineW();
    }

    // dynamically resolve NtQueryInformationProcess from ntdll
    typedef LONG(NTAPI* NtQIP_t)(HANDLE, ULONG, PVOID, ULONG, PULONG);
    static NtQIP_t pNtQIP = (NtQIP_t)GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtQueryInformationProcess");
    if (!pNtQIP) return L"";

    // ProcessBasicInformation (class 0) gives us the PEB address
    struct { PVOID R1; PVOID PebBaseAddress; PVOID R2[2]; ULONG_PTR UniqueProcessId; PVOID R3; } pbi{};
    ULONG ret_len = 0;
    if (pNtQIP(hProc, 0, &pbi, sizeof(pbi), &ret_len) != 0 || !pbi.PebBaseAddress)
        return L"";

    // PEB.ProcessParameters offset: 0x20 on x64, 0x10 on x86
    // RTL_USER_PROCESS_PARAMETERS.CommandLine offset: 0x70 on x64, 0x40 on x86
#ifdef _WIN64
    constexpr SIZE_T peb_params_off = 0x20;
    constexpr SIZE_T cmdline_off = 0x70;
    constexpr SIZE_T us_ptr_off = 8; // UNICODE_STRING: USHORT Len, USHORT MaxLen, 4-pad, PWSTR(8)
#else
    constexpr SIZE_T peb_params_off = 0x10;
    constexpr SIZE_T cmdline_off = 0x40;
    constexpr SIZE_T us_ptr_off = 4; // UNICODE_STRING: USHORT Len, USHORT MaxLen, PWSTR(4)
#endif

    // read ProcessParameters pointer from PEB
    PVOID params_ptr = nullptr;
    SIZE_T n = 0;
    if (!ReadProcessMemory(hProc, (BYTE*)pbi.PebBaseAddress + peb_params_off, &params_ptr, sizeof(params_ptr), &n) || !params_ptr)
        return L"";

    // read CommandLine UNICODE_STRING: { USHORT Length, USHORT MaximumLength, [pad], PWSTR Buffer }
    BYTE us_buf[16]{};
    if (!ReadProcessMemory(hProc, (BYTE*)params_ptr + cmdline_off, us_buf, sizeof(us_buf), &n))
        return L"";

    USHORT length = *(USHORT*)us_buf;
    PVOID buffer = *(PVOID*)(us_buf + us_ptr_off);
    if (!buffer || length == 0 || length > 32768) return L"";

    // read the actual command line string
    std::wstring cmdline(length / sizeof(wchar_t), L'\0');
    if (!ReadProcessMemory(hProc, buffer, &cmdline[0], length, &n))
        return L"";

    return cmdline;
}

// dump the full process tree of the current process to steam_api(64).dll.txt
// this runs very early in DLL_PROCESS_ATTACH, before any game code or SteamAPI_Init
static void dump_process_tree()
{
    // get our DLL path to determine where to write the file and compute relative paths
    static const char anchor = 0;
    HMODULE our_module = nullptr;
    GetModuleHandleExW(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        reinterpret_cast<LPCWSTR>(&anchor),
        &our_module
    );
    if (!our_module) return;

    wchar_t dll_path_w[MAX_PATH]{};
    if (!GetModuleFileNameW(our_module, dll_path_w, MAX_PATH)) return;

    // derive output file path: <dll_name>.txt next to the DLL
    std::wstring out_path(dll_path_w);
    out_path += L".txt";

    // derive DLL directory for relative path computation
    std::wstring dll_dir(dll_path_w);
    auto last_sep = dll_dir.find_last_of(L"\\/");
    if (last_sep != std::wstring::npos) dll_dir.resize(last_sep + 1);

    // get DLL filename for the header
    const wchar_t* dll_name = (last_sep != std::wstring::npos) ? &dll_path_w[last_sep + 1] : dll_path_w;

    // walk the process tree: current → parent → grandparent → ...
    struct ProcessInfo {
        DWORD pid;
        DWORD parent_pid;
        std::wstring exe_name;
        std::wstring full_path;
        std::wstring cmdline;
        std::string start_time; // formatted creation timestamp
        struct RendererEntry {
            std::string label;
            std::wstring full_path;
            bool is_proxy;
        };
        std::vector<RendererEntry> renderers; // loaded renderer/overlay DLLs (for parent processes)
    };
    std::vector<ProcessInfo> tree;

    // DLLs to look for when scanning parent process modules
    struct { const wchar_t* dll; const char* label; } renderer_dlls[] = {
        { L"ddraw.dll",      "DirectDraw" },
        { L"d3dimm.dll",     "Direct3D Immediate Mode" },
        { L"d3d8.dll",       "DirectX 8" },
        { L"d3d9.dll",       "DirectX 9" },
        { L"d3d10.dll",      "DirectX 10" },
        { L"d3d10_1.dll",    "DirectX 10.1" },
        { L"d3d11.dll",      "DirectX 11" },
        { L"d3d12.dll",      "DirectX 12" },
        { L"vulkan-1.dll",   "Vulkan" },
        { L"opengl32.dll",   "OpenGL" },
        { L"dxgi.dll",       "DXGI" },
        { L"dinput8.dll",    "DirectInput 8" },
        // 3dfx Glide wrappers (dgVoodoo)
        { L"glide.dll",      "Glide (3dfx)" },
        { L"glide2x.dll",    "Glide 2x (3dfx)" },
        { L"glide3x.dll",    "Glide 3x (3dfx)" },
        { L"SpecialK32.dll", "Special K (32-bit)" },
        { L"SpecialK64.dll", "Special K (64-bit)" },
        { L"ReShade32.dll",  "ReShade (32-bit)" },
        { L"ReShade64.dll",  "ReShade (64-bit)" },
    };

    // system directories for proxy detection in parent processes
    wchar_t parent_sys_dir[MAX_PATH]{};
    GetSystemDirectoryW(parent_sys_dir, MAX_PATH);
    size_t parent_sys_len = wcslen(parent_sys_dir);
    wchar_t parent_syswow_dir[MAX_PATH]{};
    UINT parent_syswow_len = GetSystemWow64DirectoryW(parent_syswow_dir, MAX_PATH);

    // snapshot all processes
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    if (snap == INVALID_HANDLE_VALUE) return;

    // build a map of pid → (parent_pid, exe_name)
    std::map<DWORD, std::pair<DWORD, std::wstring>> proc_map;
    PROCESSENTRY32W pe{};
    pe.dwSize = sizeof(pe);
    if (Process32FirstW(snap, &pe)) {
        do {
            proc_map[pe.th32ProcessID] = { pe.th32ParentProcessID, pe.szExeFile };
        } while (Process32NextW(snap, &pe));
    }
    CloseHandle(snap);

    // walk from current process up
    DWORD current_pid = GetCurrentProcessId();
    std::set<DWORD> visited; // prevent infinite loops from pid reuse
    DWORD walk_pid = current_pid;
    while (walk_pid && visited.insert(walk_pid).second) {
        auto it = proc_map.find(walk_pid);
        if (it == proc_map.end()) break;

        ProcessInfo info{};
        info.pid = walk_pid;
        info.parent_pid = it->second.first;
        info.exe_name = it->second.second;

        // try to get full path, command line, and creation time
        // PROCESS_VM_READ needed for PEB command line reading
        HANDLE hProc = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, walk_pid);
        bool full_access = (hProc != nullptr);
        if (!hProc) hProc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, walk_pid);
        if (hProc) {
            wchar_t path_buf[MAX_PATH]{};
            DWORD path_size = MAX_PATH;
            if (QueryFullProcessImageNameW(hProc, 0, path_buf, &path_size)) {
                info.full_path = path_buf;
            }
            if (full_access) {
                info.cmdline = read_process_cmdline(hProc, walk_pid);
            }
            // get process creation time
            FILETIME ft_create{}, ft_exit{}, ft_kernel{}, ft_user{};
            if (GetProcessTimes(hProc, &ft_create, &ft_exit, &ft_kernel, &ft_user) && (ft_create.dwHighDateTime || ft_create.dwLowDateTime)) {
                SYSTEMTIME st_utc{}, st_local{};
                FileTimeToSystemTime(&ft_create, &st_utc);
                SystemTimeToTzSpecificLocalTime(nullptr, &st_utc, &st_local);
                char tbuf[64]{};
                snprintf(tbuf, sizeof(tbuf), "%04d-%02d-%02d %02d:%02d:%02d",
                    st_local.wYear, st_local.wMonth, st_local.wDay,
                    st_local.wHour, st_local.wMinute, st_local.wSecond);
                info.start_time = tbuf;
            }
            CloseHandle(hProc);
        }

        // scan loaded modules for renderer/overlay DLLs (skip current process — renderer not loaded yet at DLL_PROCESS_ATTACH)
        if (walk_pid != current_pid) {
            HANDLE mod_snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, walk_pid);
            if (mod_snap != INVALID_HANDLE_VALUE) {
                MODULEENTRY32W me{};
                me.dwSize = sizeof(me);
                if (Module32FirstW(mod_snap, &me)) {
                    do {
                        for (auto& rd : renderer_dlls) {
                            if (_wcsicmp(me.szModule, rd.dll) == 0) {
                                ProcessInfo::RendererEntry entry{};
                                entry.label = rd.label;
                                entry.full_path = me.szExePath;
                                // proxy detection: compare against system directories
                                std::wstring mod_dir(me.szExePath);
                                auto sep = mod_dir.find_last_of(L"\\/");
                                if (sep != std::wstring::npos) mod_dir.resize(sep);
                                bool in_system = (_wcsnicmp(mod_dir.c_str(), parent_sys_dir, parent_sys_len) == 0 && mod_dir.size() == parent_sys_len);
                                if (!in_system && parent_syswow_len > 0) {
                                    in_system = (_wcsnicmp(mod_dir.c_str(), parent_syswow_dir, parent_syswow_len) == 0 && mod_dir.size() == parent_syswow_len);
                                }
                                entry.is_proxy = !in_system;
                                info.renderers.push_back(std::move(entry));
                                break;
                            }
                        }
                    } while (Module32NextW(mod_snap, &me));
                }
                CloseHandle(mod_snap);
            }
        }

        tree.push_back(std::move(info));
        walk_pid = it->second.first;
    }

    // compute relative path from DLL directory
    auto make_relative = [&dll_dir](const std::wstring& full_path) -> std::wstring {
        if (full_path.empty()) return L"(unknown)";
        // case-insensitive prefix check
        if (full_path.size() >= dll_dir.size() &&
            _wcsnicmp(full_path.c_str(), dll_dir.c_str(), dll_dir.size()) == 0) {
            std::wstring rel = L".\\" + full_path.substr(dll_dir.size());
            return rel;
        }
        return full_path; // different drive/root, return absolute
    };

    // sanitize user profile path
    wchar_t profile_w[MAX_PATH]{};
    DWORD profile_len = GetEnvironmentVariableW(L"USERPROFILE", profile_w, MAX_PATH);

    auto sanitize = [&profile_w, profile_len](const std::wstring& path) -> std::wstring {
        if (profile_len == 0 || path.size() < profile_len) return path;
        if (_wcsnicmp(path.c_str(), profile_w, profile_len) == 0) {
            return L"%USERPROFILE%" + path.substr(profile_len);
        }
        return path;
    };

    // write the file
    FILE* f = _wfopen(out_path.c_str(), L"w");
    if (!f) return;

    // timestamp
    SYSTEMTIME st{};
    GetLocalTime(&st);
    fprintf(f, "=== %ls Process Tree ===\n", dll_name);
    fprintf(f, "Timestamp: %04d-%02d-%02d %02d:%02d:%02d\n",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond);

    // sanitize DLL path for display
    char dll_path_a[MAX_PATH]{};
    WideCharToMultiByte(CP_UTF8, 0, dll_path_w, -1, dll_path_a, MAX_PATH, nullptr, nullptr);
    char profile_a[MAX_PATH]{};
    GetEnvironmentVariableA("USERPROFILE", profile_a, MAX_PATH);
    size_t plen = strlen(profile_a);
    std::string dll_display(dll_path_a);
    if (plen > 0 && dll_display.size() >= plen && _strnicmp(dll_display.c_str(), profile_a, plen) == 0) {
        dll_display.replace(0, plen, "%USERPROFILE%");
    }
    fprintf(f, "DLL Path: %s\n\n", dll_display.c_str());

    // print tree (first entry is current process, last is the topmost ancestor we could reach)
    for (size_t i = 0; i < tree.size(); ++i) {
        auto& p = tree[i];
        bool is_current = (p.pid == current_pid);

        std::wstring sanitized_path = sanitize(p.full_path);
        std::wstring rel = make_relative(p.full_path);
        std::wstring sanitized_rel = sanitize(rel);

        char name_a[MAX_PATH]{};
        WideCharToMultiByte(CP_UTF8, 0, p.exe_name.c_str(), -1, name_a, MAX_PATH, nullptr, nullptr);
        char path_a[MAX_PATH * 2]{};
        WideCharToMultiByte(CP_UTF8, 0, sanitized_path.c_str(), -1, path_a, sizeof(path_a), nullptr, nullptr);
        char rel_a[MAX_PATH * 2]{};
        WideCharToMultiByte(CP_UTF8, 0, sanitized_rel.c_str(), -1, rel_a, sizeof(rel_a), nullptr, nullptr);

        fprintf(f, "[PID %lu] %s%s\n", p.pid, name_a, is_current ? "  (current process)" : "");
        fprintf(f, "  Path: %s\n", path_a);
        fprintf(f, "  Relative: %s\n", rel_a);
        if (!p.start_time.empty()) {
            fprintf(f, "  Started: %s\n", p.start_time.c_str());
        }
        if (!p.cmdline.empty()) {
            std::wstring sanitized_cmd = sanitize(p.cmdline);
            int cmd_size = WideCharToMultiByte(CP_UTF8, 0, sanitized_cmd.c_str(), -1, nullptr, 0, nullptr, nullptr);
            if (cmd_size > 0) {
                std::string cmd_a(cmd_size - 1, '\0');
                WideCharToMultiByte(CP_UTF8, 0, sanitized_cmd.c_str(), -1, &cmd_a[0], cmd_size, nullptr, nullptr);
                fprintf(f, "  CmdLine: %s\n", cmd_a.c_str());
            }
        }
        if (is_current) {
            fprintf(f, "  Renderers: (pending - detected after init)\n");
        } else if (!p.renderers.empty()) {
            if (p.renderers.size() == 1) {
                auto& r = p.renderers[0];
                std::wstring san_rpath = sanitize(r.full_path);
                char rpath_a[MAX_PATH * 2]{};
                WideCharToMultiByte(CP_UTF8, 0, san_rpath.c_str(), -1, rpath_a, sizeof(rpath_a), nullptr, nullptr);
                if (r.is_proxy) {
                    fprintf(f, "  Renderers: %s [PROXY]\n", r.label.c_str());
                } else {
                    fprintf(f, "  Renderers: %s\n", r.label.c_str());
                }
                fprintf(f, "    %s\n", rpath_a);
            } else {
                fprintf(f, "  Renderers:\n");
                for (auto& r : p.renderers) {
                    std::wstring san_rpath = sanitize(r.full_path);
                    char rpath_a[MAX_PATH * 2]{};
                    WideCharToMultiByte(CP_UTF8, 0, san_rpath.c_str(), -1, rpath_a, sizeof(rpath_a), nullptr, nullptr);
                    if (r.is_proxy) {
                        fprintf(f, "    %s [PROXY]\n", r.label.c_str());
                    } else {
                        fprintf(f, "    %s\n", r.label.c_str());
                    }
                    fprintf(f, "      %s\n", rpath_a);
                }
            }
        }
        fprintf(f, "\n");

        PRINT_DEBUG("process tree [PID %lu]: %s%s | path: %s", p.pid, name_a,
            is_current ? " (current)" : "", path_a);
    }

    fclose(f);
}

// append detected renderer(s) and third-party modules for every process that loaded us
// called from SteamAPI_RunCallbacks (first call only), when the game's renderer is initialized
void append_renderer_info()
{
    // get our DLL path to derive the output file path
    static const char anchor = 0;
    HMODULE our_module = nullptr;
    GetModuleHandleExW(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        reinterpret_cast<LPCWSTR>(&anchor),
        &our_module
    );
    if (!our_module) return;

    wchar_t dll_path_w[MAX_PATH]{};
    if (!GetModuleFileNameW(our_module, dll_path_w, MAX_PATH)) return;

    std::wstring out_path(dll_path_w);
    out_path += L".txt";

    // --- Environment / compatibility layer detection ---
    struct EnvCheck { const char* var; const char* label; };
    // check if running under Wine/Proton by looking for wine_get_version in ntdll
    bool is_wine = false;
    std::string wine_version;
    HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
    if (ntdll) {
        typedef const char* (*wine_get_version_t)();
        auto wine_ver = (wine_get_version_t)GetProcAddress(ntdll, "wine_get_version");
        if (wine_ver) {
            is_wine = true;
            const char* v = wine_ver();
            if (v) wine_version = v;
        }
    }

    // detect specific compatibility layers / launchers via environment variables
    std::vector<std::string> env_info;

    if (is_wine) {
        std::string wine_label = "Wine";
        if (!wine_version.empty()) wine_label += " " + wine_version;
        env_info.push_back(wine_label);
    }

    // Proton (Steam Play)
    char env_buf[512]{};
    if (GetEnvironmentVariableA("STEAM_COMPAT_DATA_PATH", env_buf, sizeof(env_buf))) {
        std::string proton_label = "Proton (Steam Play)";
        char proton_ver[256]{};
        if (GetEnvironmentVariableA("PROTON_VERSION", proton_ver, sizeof(proton_ver)))
            proton_label += std::string(" ") + proton_ver;
        env_info.push_back(proton_label);
    }

    // Lutris
    if (GetEnvironmentVariableA("LUTRIS_GAME_SLUG", env_buf, sizeof(env_buf)))
        env_info.push_back("Lutris");

    // Bottles
    if (GetEnvironmentVariableA("BOTTLES_ENV", env_buf, sizeof(env_buf)) ||
        GetEnvironmentVariableA("FLATPAK_ID", env_buf, sizeof(env_buf)) && strstr(env_buf, "bottles"))
        env_info.push_back("Bottles");

    // PlayOnLinux
    if (GetEnvironmentVariableA("PLAYONLINUX", env_buf, sizeof(env_buf)) ||
        GetEnvironmentVariableA("POL_WINEVERSION", env_buf, sizeof(env_buf)))
        env_info.push_back("PlayOnLinux");

    // CrossOver (CodeWeavers)
    if (GetEnvironmentVariableA("CX_BOTTLE", env_buf, sizeof(env_buf)) ||
        GetEnvironmentVariableA("CX_ROOT", env_buf, sizeof(env_buf)))
        env_info.push_back("CrossOver");

    // Heroic Games Launcher
    if (GetEnvironmentVariableA("HEROIC_APP_NAME", env_buf, sizeof(env_buf)) ||
        GetEnvironmentVariableA("STORE", env_buf, sizeof(env_buf)) && (strstr(env_buf, "legendary") || strstr(env_buf, "gog")))
        env_info.push_back("Heroic Games Launcher");

    // GameScope (Steam Deck compositing)
    if (GetEnvironmentVariableA("GAMESCOPE_WAYLAND_DISPLAY", env_buf, sizeof(env_buf)))
        env_info.push_back("GameScope");

    // MangoHud
    if (GetEnvironmentVariableA("MANGOHUD", env_buf, sizeof(env_buf)))
        env_info.push_back("MangoHud");

    // Steam Runtime
    if (GetEnvironmentVariableA("STEAM_RUNTIME", env_buf, sizeof(env_buf)))
        env_info.push_back(std::string("Steam Runtime: ") + env_buf);

    // --- Translation layer detection (DXVK, VKD3D, WineD3D) ---
    std::vector<std::string> translation_info;

    if (is_wine) {
        // DXVK: check for DXVK-specific exports on dxgi.dll or env
        bool dxvk_detected = false;
        HMODULE dxgi_mod = GetModuleHandleW(L"dxgi.dll");
        if (dxgi_mod && GetProcAddress(dxgi_mod, "DXVK_GetInstanceExtensions")) {
            dxvk_detected = true;
        }
        if (!dxvk_detected && GetEnvironmentVariableA("DXVK_LOG_LEVEL", env_buf, sizeof(env_buf))) {
            dxvk_detected = true;
        }
        if (!dxvk_detected && GetEnvironmentVariableA("DXVK_STATE_CACHE", env_buf, sizeof(env_buf))) {
            dxvk_detected = true;
        }
        if (dxvk_detected) {
            translation_info.push_back("DXVK (D3D9/D3D10/D3D11 -> Vulkan)");
        }

        // VKD3D-proton: D3D12 -> Vulkan
        bool vkd3d_detected = false;
        HMODULE d3d12_mod = GetModuleHandleW(L"d3d12.dll");
        if (d3d12_mod && GetProcAddress(d3d12_mod, "vkd3d_create_instance")) {
            vkd3d_detected = true;
        }
        if (!vkd3d_detected && GetEnvironmentVariableA("VKD3D_LOG_LEVEL", env_buf, sizeof(env_buf))) {
            vkd3d_detected = true;
        }
        if (vkd3d_detected) {
            translation_info.push_back("VKD3D-proton (D3D12 -> Vulkan)");
        }

        // WineD3D: if D3D is loaded but neither DXVK nor VKD3D, it's WineD3D (OpenGL-based)
        bool has_d3d = GetModuleHandleW(L"d3d9.dll") || GetModuleHandleW(L"d3d10.dll") ||
                       GetModuleHandleW(L"d3d11.dll") || GetModuleHandleW(L"d3d12.dll");
        if (has_d3d && !dxvk_detected && !vkd3d_detected) {
            translation_info.push_back("WineD3D (D3D -> OpenGL)");
        }

        // Gallium Nine: native D3D9 on Mesa
        if (GetEnvironmentVariableA("WINE_NINE_NATIVE", env_buf, sizeof(env_buf)) ||
            GetModuleHandleW(L"d3d9-nine.dll")) {
            translation_info.push_back("Gallium Nine (native D3D9 on Mesa)");
        }

        // Zink: OpenGL -> Vulkan (Mesa driver)
        if (GetEnvironmentVariableA("MESA_LOADER_DRIVER_OVERRIDE", env_buf, sizeof(env_buf)) && strstr(env_buf, "zink")) {
            translation_info.push_back("Zink (OpenGL -> Vulkan via Mesa)");
        }
    }

    // --- Vulkan layer detection (ReShade, vkBasalt as implicit/explicit Vulkan layers) ---
    std::vector<std::string> vk_layer_info;

    // check VK_INSTANCE_LAYERS env var (explicit layer activation)
    char vk_layers_buf[2048]{};
    if (GetEnvironmentVariableA("VK_INSTANCE_LAYERS", vk_layers_buf, sizeof(vk_layers_buf))) {
        // colon/semicolon-separated list of layer names
        if (strstr(vk_layers_buf, "VK_LAYER_reshade"))
            vk_layer_info.push_back("ReShade (Vulkan layer via VK_INSTANCE_LAYERS)");
        if (strstr(vk_layers_buf, "VK_LAYER_vkBasalt") || strstr(vk_layers_buf, "vkBasalt"))
            vk_layer_info.push_back("vkBasalt (Vulkan layer via VK_INSTANCE_LAYERS)");
    }

    // check ENABLE_VKBASALT env var
    if (GetEnvironmentVariableA("ENABLE_VKBASALT", env_buf, sizeof(env_buf)))
        vk_layer_info.push_back("vkBasalt (enabled via ENABLE_VKBASALT)");

    // check Windows registry for ReShade implicit Vulkan layer
    {
        HKEY layers_key = nullptr;
        const wchar_t* reg_paths[] = {
            L"SOFTWARE\\Khronos\\Vulkan\\ImplicitLayers",
            L"SOFTWARE\\Khronos\\Vulkan\\ExplicitLayers",
        };
        for (auto& reg_path : reg_paths) {
            // check both HKLM and HKCU
            HKEY roots[] = { HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER };
            for (auto root : roots) {
                if (RegOpenKeyExW(root, reg_path, 0, KEY_READ, &layers_key) == ERROR_SUCCESS) {
                    DWORD idx = 0;
                    wchar_t value_name[1024]{};
                    DWORD name_len = sizeof(value_name) / sizeof(wchar_t);
                    while (RegEnumValueW(layers_key, idx++, value_name, &name_len, nullptr, nullptr, nullptr, nullptr) == ERROR_SUCCESS) {
                        // value_name is the path to the JSON manifest
                        if (wcsstr(value_name, L"reshade") || wcsstr(value_name, L"ReShade")) {
                            bool is_implicit = (wcsstr(reg_path, L"Implicit") != nullptr);
                            char manifest_a[1024]{};
                            WideCharToMultiByte(CP_UTF8, 0, value_name, -1, manifest_a, sizeof(manifest_a), nullptr, nullptr);
                            vk_layer_info.push_back(std::string("ReShade (Vulkan ") +
                                (is_implicit ? "implicit" : "explicit") + " layer: " + manifest_a + ")");
                        }
                        name_len = sizeof(value_name) / sizeof(wchar_t);
                    }
                    RegCloseKey(layers_key);
                }
            }
        }
    }

    // renderer and overlay DLLs to check (name, label)
    struct { const wchar_t* dll; const char* label; } renderers[] = {
        { L"ddraw.dll",      "DirectDraw" },
        { L"d3dimm.dll",     "Direct3D Immediate Mode" },
        { L"d3d8.dll",       "DirectX 8" },
        { L"d3d9.dll",       "DirectX 9" },
        { L"d3d10.dll",      "DirectX 10" },
        { L"d3d10_1.dll",    "DirectX 10.1" },
        { L"d3d11.dll",      "DirectX 11" },
        { L"d3d12.dll",      "DirectX 12" },
        { L"d3d12core.dll",  "DirectX 12 Core (Agility SDK)" },
        { L"vulkan-1.dll",   "Vulkan" },
        { L"opengl32.dll",   "OpenGL" },
        { L"dxgi.dll",       "DXGI" },
        { L"libEGL.dll",     "EGL (ANGLE)" },
        { L"libGLESv2.dll",  "OpenGL ES (ANGLE)" },
        { L"dinput8.dll",    "DirectInput 8" },
        // 3dfx Glide wrappers (dgVoodoo)
        { L"glide.dll",      "Glide (3dfx)" },
        { L"glide2x.dll",    "Glide 2x (3dfx)" },
        { L"glide3x.dll",    "Glide 3x (3dfx)" },
        // Special K — global injection (SKIF) or local install
        { L"SpecialK32.dll", "Special K (32-bit)" },
        { L"SpecialK64.dll", "Special K (64-bit)" },
        // ReShade — standalone or loaded as SK plugin
        { L"ReShade32.dll",  "ReShade (32-bit)" },
        { L"ReShade64.dll",  "ReShade (64-bit)" },
    };

    // get system directory for proxy detection
    wchar_t sys_dir[MAX_PATH]{};
    GetSystemDirectoryW(sys_dir, MAX_PATH);
    size_t sys_dir_len = wcslen(sys_dir);
    // also check SysWOW64 for 32-bit DLLs on 64-bit OS
    wchar_t syswow_dir[MAX_PATH]{};
    UINT syswow_len = GetSystemWow64DirectoryW(syswow_dir, MAX_PATH);

    // sanitize user profile path in renderer DLL paths
    wchar_t profile_w[MAX_PATH]{};
    DWORD profile_len = GetEnvironmentVariableW(L"USERPROFILE", profile_w, MAX_PATH);

    // known proxy identifiers: export name -> proxy label
    struct ProxySignature { const char* export_name; const char* proxy_label; };
    ProxySignature proxy_sigs[] = {
        { "DXVK_GetInstanceExtensions",  "DXVK" },
        { "vkd3d_create_instance",       "VKD3D-proton" },
        // Special K exports (proxy DLLs like dxgi.dll, d3d11.dll, dinput8.dll replaced by SK)
        { "SK_GetVersionStr",            "Special K" },
        { "SK_GetDLLRole",               "Special K" },
        { "SK_GetPlugInDirectory",       "Special K" },
        // ReShade exports (proxy DLLs or standalone)
        { "ReShadeVersion",              "ReShade" },
        { "ReShadeRegisterAddon",        "ReShade" },
        { "ENBGetVersion",               "ENB Series" },
        { "dgVoodooVersion",             "dgVoodoo" },
        { "D3D8_GetDirect3D",            "d3d8to9" },
    };

    struct DetectedRenderer {
        std::string label;
        std::string full_path;
        bool is_proxy;
        std::string proxy_label;
        std::string category;    // GAME, OVERLAY, TRANSLATION, INFRA, INPUT
        std::string annotation;  // e.g., "loaded by Special K for overlay rendering"
    };
    std::vector<DetectedRenderer> detected;

    for (auto& r : renderers) {
        HMODULE mod = GetModuleHandleW(r.dll);
        if (!mod) continue;

        DetectedRenderer entry{};
        entry.label = r.label;

        // get full path of the loaded DLL
        wchar_t mod_path[MAX_PATH]{};
        if (GetModuleFileNameW(mod, mod_path, MAX_PATH)) {
            // sanitize user profile path
            std::wstring path_w(mod_path);
            if (profile_len > 0 && path_w.size() >= profile_len &&
                _wcsnicmp(path_w.c_str(), profile_w, profile_len) == 0) {
                path_w = L"%USERPROFILE%" + path_w.substr(profile_len);
            }
            char path_a[MAX_PATH * 2]{};
            WideCharToMultiByte(CP_UTF8, 0, path_w.c_str(), -1, path_a, sizeof(path_a), nullptr, nullptr);
            entry.full_path = path_a;

            // check if DLL is loaded from outside system directories = proxy
            // extract directory from the loaded DLL's full path
            std::wstring mod_dir(mod_path);
            auto sep = mod_dir.find_last_of(L"\\/");
            if (sep != std::wstring::npos) mod_dir.resize(sep);

            bool in_system = (_wcsnicmp(mod_dir.c_str(), sys_dir, sys_dir_len) == 0 && mod_dir.size() == sys_dir_len);
            if (!in_system && syswow_len > 0) {
                in_system = (_wcsnicmp(mod_dir.c_str(), syswow_dir, syswow_len) == 0 && mod_dir.size() == syswow_len);
            }

            if (!in_system) {
                entry.is_proxy = true;
                // identify the proxy by checking known exports
                for (auto& sig : proxy_sigs) {
                    if (GetProcAddress(mod, sig.export_name)) {
                        entry.proxy_label = sig.proxy_label;
                        break;
                    }
                }
                if (entry.proxy_label.empty()) {
                    entry.proxy_label = "unknown proxy";
                }
            }
        } else {
            char dll_a[MAX_PATH]{};
            WideCharToMultiByte(CP_UTF8, 0, r.dll, -1, dll_a, MAX_PATH, nullptr, nullptr);
            entry.full_path = dll_a;
        }

        detected.push_back(std::move(entry));
    }

    // --- classify each detected entry ---
    // first pass: identify overlays
    bool sk_present = false;
    bool reshade_present = false;
    std::string reshade_path_str;
    for (auto& d : detected) {
        if (d.label.find("Special K") != std::string::npos) {
            sk_present = true;
        }
        if (d.label.find("ReShade") != std::string::npos) {
            reshade_present = true;
            reshade_path_str = d.full_path;
        }
    }

    // find the lowest and highest system (non-proxy) D3D versions
    int lowest_system_d3d = 0; // 8,9,10,11,12
    int highest_system_d3d = 0;
    bool has_system_vulkan = false;
    bool has_system_opengl = false;
    for (auto& d : detected) {
        if (d.is_proxy) continue;
        int ver = 0;
        if (d.label == "DirectX 8")       ver = 8;
        else if (d.label == "DirectX 9")  ver = 9;
        else if (d.label == "DirectX 10" || d.label == "DirectX 10.1") ver = 10;
        else if (d.label == "DirectX 11") ver = 11;
        else if (d.label == "DirectX 12" || d.label == "DirectX 12 Core (Agility SDK)") ver = 12;
        else if (d.label == "Vulkan")  { has_system_vulkan = true; continue; }
        else if (d.label == "OpenGL")  { has_system_opengl = true; continue; }
        if (ver) {
            if (!lowest_system_d3d || ver < lowest_system_d3d) lowest_system_d3d = ver;
            if (ver > highest_system_d3d) highest_system_d3d = ver;
        }
    }

    // second pass: classify
    for (auto& d : detected) {
        // overlay tools
        if (d.label.find("Special K") != std::string::npos) {
            d.category = "OVERLAY";
            d.annotation = "overlay renders via DirectX 11";
            continue;
        }
        if (d.label.find("ReShade") != std::string::npos) {
            d.category = "OVERLAY";
            if (d.full_path.find("SpecialK") != std::string::npos ||
                d.full_path.find("PlugIns") != std::string::npos ||
                d.full_path.find("Special K") != std::string::npos) {
                d.annotation = "loaded as Special K plugin";
            }
            continue;
        }

        // proxy'd DLLs (translation layers / overlay hooks)
        if (d.is_proxy) {
            if (d.proxy_label == "dgVoodoo") {
                d.category = "TRANSLATION";
                d.annotation = "translates " + d.label + " -> DirectX 11";
            } else if (d.proxy_label == "DXVK") {
                d.category = "TRANSLATION";
                d.annotation = "translates " + d.label + " -> Vulkan";
            } else if (d.proxy_label == "VKD3D-proton") {
                d.category = "TRANSLATION";
                d.annotation = "translates DirectX 12 -> Vulkan";
            } else if (d.proxy_label == "d3d8to9") {
                d.category = "TRANSLATION";
                d.annotation = "translates DirectX 8 -> DirectX 9";
            } else if (d.proxy_label == "ENB Series") {
                d.category = "OVERLAY";
                d.annotation = "post-processing (hooks " + d.label + ")";
            } else if (d.proxy_label == "Special K") {
                d.category = "OVERLAY";
                d.annotation = "Special K proxy (hooks " + d.label + ")";
            } else if (d.proxy_label == "ReShade") {
                d.category = "OVERLAY";
                d.annotation = "ReShade proxy (hooks " + d.label + ")";
            } else {
                d.category = "TRANSLATION";
                d.annotation = "proxy: " + d.proxy_label;
            }
            continue;
        }

        // infrastructure (never primary renderers)
        if (d.label == "DXGI") {
            d.category = "INFRA";
            continue;
        }
        if (d.label == "DirectInput 8") {
            d.category = "INPUT";
            continue;
        }
        if (d.label == "EGL (ANGLE)" || d.label == "OpenGL ES (ANGLE)") {
            d.category = "INFRA";
            continue;
        }

        // DirectDraw / D3D Immediate Mode from system = legacy, likely game renderer
        if (d.label == "DirectDraw" || d.label == "Direct3D Immediate Mode") {
            d.category = "GAME";
            continue;
        }

        // Glide from system doesn't exist (no system glide DLL) - should be caught by proxy
        if (d.label.find("Glide") != std::string::npos) {
            d.category = "GAME";
            continue;
        }

        // system D3D11 when SK is present and the game uses a different renderer
        // SK always loads D3D11 for its overlay, even in D3D9/D3D12/Vulkan/OpenGL games
        if (d.label == "DirectX 11" && sk_present &&
            ((lowest_system_d3d && lowest_system_d3d < 11) ||
             highest_system_d3d > 11 ||
             has_system_vulkan || has_system_opengl)) {
            d.category = "INFRA";
            d.annotation = "loaded by Special K for overlay rendering";
            continue;
        }

        // everything else from system = game renderer
        d.category = "GAME";
    }

    // --- Third-party tool detection (non-renderer modules) ---
    struct DetectedTool {
        std::string label;
        std::string type;
    };
    std::vector<DetectedTool> detected_tools;
    {
        struct { const wchar_t* name; const char* label; const char* type; } known_tools[] = {
            // --- injectors / post-processors ---
            #if defined(_WIN64)
            { L"SpecialK64.dll",            "Special K",            "injector" },
            { L"ReShade64.dll",             "ReShade",              "post-processor" },
            #else
            { L"SpecialK32.dll",            "Special K",            "injector" },
            { L"ReShade32.dll",             "ReShade",              "post-processor" },
            #endif
            { L"d3dcompiler_46e.dll",       "ENB Series",           "post-processor" },

            // --- recording / streaming ---
            #if defined(_WIN64)
            { L"nvspcap64.dll",             "NVIDIA ShadowPlay",    "recording" },
            { L"graphics-hook64.dll",       "OBS Game Capture",     "recording" },
            { L"fraps64.dll",               "Fraps",                "recording" },
            { L"MedalHook64.dll",           "Medal.tv",             "recording" },
            { L"bdcam64.dll",               "Bandicam",             "recording" },
            { L"Action64.dll",              "Mirillis Action",      "recording" },
            { L"XSplit.Core64.dll",         "XSplit",               "recording" },
            { L"d3dgear64.dll",             "D3DGear",              "recording" },
            #else
            { L"nvspcap.dll",               "NVIDIA ShadowPlay",    "recording" },
            { L"graphics-hook32.dll",       "OBS Game Capture",     "recording" },
            { L"fraps32.dll",               "Fraps",                "recording" },
            { L"MedalHook.dll",             "Medal.tv",             "recording" },
            { L"bdcam32.dll",               "Bandicam",             "recording" },
            { L"Action.dll",                "Mirillis Action",      "recording" },
            { L"XSplit.Core.dll",           "XSplit",               "recording" },
            { L"d3dgear.dll",               "D3DGear",              "recording" },
            #endif
            { L"Streamlabs.dll",            "Streamlabs",           "recording" },

            // --- monitoring ---
            { L"RTSSHooks64.dll",           "RTSS",                 "monitoring" },
            { L"RTSSHooks.dll",             "RTSS",                 "monitoring" },
            #if defined(_WIN64)
            { L"fpshook64.dll",             "FPS Monitor",          "monitoring" },
            { L"PresentMon64.dll",          "Intel PresentMon",     "monitoring" },
            #else
            { L"fpshook.dll",               "FPS Monitor",          "monitoring" },
            { L"PresentMon32.dll",          "Intel PresentMon",     "monitoring" },
            #endif

            // --- store overlays ---
            { L"GameOverlayRenderer64.dll", "Steam Overlay",        "store overlay" },
            { L"GameOverlayRenderer.dll",   "Steam Overlay",        "store overlay" },
            { L"DiscordHook64.dll",         "Discord",              "store overlay" },
            { L"DiscordHook.dll",           "Discord",              "store overlay" },
            #if defined(_WIN64)
            { L"EOSOVH-Win64-Shipping.dll", "Epic Online Services", "store overlay" },
            { L"Galaxy64.dll",              "GOG Galaxy",           "store overlay" },
            { L"GalaxyOverlayRenderer64.dll", "GOG Galaxy",         "store overlay" },
            { L"igo64.dll",                 "EA App / Origin",      "store overlay" },
            { L"uplay_r2_loader64.dll",     "Ubisoft Connect",      "store overlay" },
            { L"upc_r2_loader64.dll",       "Ubisoft Connect",      "store overlay" },
            #else
            { L"EOSOVH-Win32-Shipping.dll", "Epic Online Services", "store overlay" },
            { L"Galaxy.dll",                "GOG Galaxy",           "store overlay" },
            { L"GalaxyOverlayRenderer.dll", "GOG Galaxy",           "store overlay" },
            { L"igo32.dll",                 "EA App / Origin",      "store overlay" },
            { L"uplay_r2_loader.dll",       "Ubisoft Connect",      "store overlay" },
            { L"upc_r2_loader.dll",         "Ubisoft Connect",      "store overlay" },
            #endif

            // --- GPU vendor software ---
            #if defined(_WIN64)
            { L"aaborern64.dll",            "AMD Adrenalin",        "gpu vendor" },
            { L"atiumd64.dll",              "AMD Display Driver",   "gpu vendor" },
            #else
            { L"aaborern.dll",              "AMD Adrenalin",        "gpu vendor" },
            { L"atiumdag.dll",              "AMD Display Driver",   "gpu vendor" },
            #endif
            { L"RadeonSoftware.dll",        "AMD Software",         "gpu vendor" },

            // --- system / platform overlays ---
            { L"GameBar.dll",               "Xbox Game Bar",        "system overlay" },
            { L"GameBarPresenceWriter.dll", "Xbox Game Bar",        "system overlay" },
            { L"SSOverlay64.dll",           "Samsung Gaming Hub",   "system overlay" },
            { L"SSOverlay.dll",             "Samsung Gaming Hub",   "system overlay" },
            { L"AcLayer.dll",               "Windows Compatibility","system overlay" },

            // --- gaming platforms / launchers ---
            { L"OWClient.dll",              "Overwolf",             "platform" },
            { L"OWExplorer.dll",            "Overwolf",             "platform" },
            #if defined(_WIN64)
            { L"ltc_game64.dll",            "Playnite",             "platform" },
            #else
            { L"ltc_game32.dll",            "Playnite",             "platform" },
            #endif

            // --- peripheral software ---
            #if defined(_WIN64)
            { L"Nahimic2OSD64.dll",         "Nahimic",              "peripheral" },
            { L"LogiOverlay64.dll",         "Logitech G Hub",       "peripheral" },
            { L"iCUEOverlay64.dll",         "Corsair iCUE",         "peripheral" },
            { L"SteelSeriesGG64.dll",       "SteelSeries GG",       "peripheral" },
            { L"RzChromaSDK64.dll",         "Razer Chroma",         "peripheral" },
            #else
            { L"Nahimic2OSD.dll",           "Nahimic",              "peripheral" },
            { L"LogiOverlay.dll",           "Logitech G Hub",       "peripheral" },
            { L"iCUEOverlay.dll",           "Corsair iCUE",         "peripheral" },
            { L"SteelSeriesGG.dll",         "SteelSeries GG",       "peripheral" },
            { L"RzChromaSDK.dll",           "Razer Chroma",         "peripheral" },
            #endif
            { L"NahimicOSD.dll",            "Nahimic",              "peripheral" },

            // --- communication ---
            #if defined(_WIN64)
            { L"mumble_ol_x64.dll",         "Mumble",               "communication" },
            { L"ts3overlay_hook_x64.dll",   "TeamSpeak",            "communication" },
            #else
            { L"mumble_ol.dll",             "Mumble",               "communication" },
            { L"ts3overlay_hook_x86.dll",   "TeamSpeak",            "communication" },
            #endif

            // --- VR ---
            { L"openvr_api.dll",            "SteamVR",              "vr" },
            #if defined(_WIN64)
            { L"vrclient_x64.dll",          "SteamVR Client",       "vr" },
            { L"LibOVRRT64_1.dll",          "Oculus Runtime",       "vr" },
            #else
            { L"vrclient.dll",              "SteamVR Client",       "vr" },
            { L"LibOVRRT32_1.dll",          "Oculus Runtime",       "vr" },
            #endif
            { L"OculusXRPlugin.dll",        "Oculus/Meta",          "vr" },

            // --- anti-cheat (informational) ---
            #if defined(_WIN64)
            { L"EasyAntiCheat_x64.dll",     "EasyAntiCheat",        "anti-cheat" },
            { L"BEService_x64.dll",         "BattlEye",             "anti-cheat" },
            { L"BEClient_x64.dll",          "BattlEye",             "anti-cheat" },
            #else
            { L"EasyAntiCheat_x86.dll",     "EasyAntiCheat",        "anti-cheat" },
            { L"BEService_x86.dll",         "BattlEye",             "anti-cheat" },
            { L"BEClient_x86.dll",          "BattlEye",             "anti-cheat" },
            #endif
            { L"easyanticheat.dll",         "EasyAntiCheat",        "anti-cheat" },
            { L"vanguard.dll",              "Vanguard",             "anti-cheat" },

            // --- modding / script hooks ---
            { L"ScriptHookV.dll",           "ScriptHookV",          "modding" },
            { L"ScriptHookRDR2.dll",        "ScriptHookRDR2",       "modding" },
            { L"ScriptHook.dll",            "ScriptHook",           "modding" },
            { L"ScriptHookDotNet.dll",      "ScriptHookDotNet",     "modding" },
            #if defined(_WIN64)
            { L"version.dll",               "ASI Loader",           "modding" },
            #else
            { L"version.dll",               "ASI Loader",           "modding" },
            #endif
        };
        std::set<std::string> tool_seen;
        for (auto& entry : known_tools) {
            if (GetModuleHandleW(entry.name) && tool_seen.insert(entry.label).second) {
                detected_tools.push_back({ entry.label, entry.type });
                PRINT_DEBUG("detected tool [%s]: %s (via %ls)", entry.type, entry.label, entry.name);
            }
        }

        // check proxy DLLs for Special K or ReShade exports
        const wchar_t* proxy_tool_dlls[] = {
            L"dxgi.dll", L"d3d11.dll", L"d3d12.dll", L"d3d10_1.dll", L"d3d10.dll", L"d3d9.dll",
            L"d3d8.dll", L"ddraw.dll", L"dinput8.dll", L"dinput.dll", L"winmm.dll",
            L"OpenGL32.dll", L"version.dll", L"dsound.dll", L"wininet.dll", L"winhttp.dll",
            L"xinput1_1.dll", L"xinput1_2.dll", L"xinput1_3.dll", L"xinput1_4.dll",
            L"xinput9_1_0.dll", L"xinputuap.dll",
            L"binkw32.dll", L"bink2w32.dll", L"binkw64.dll", L"bink2w64.dll",
            L"vorbisFile.dll", L"msacm32.dll", L"msvfw32.dll", L"xlive.dll"
        };
        for (auto dll_name : proxy_tool_dlls) {
            HMODULE hMod = GetModuleHandleW(dll_name);
            if (!hMod) continue;
            if (GetProcAddress(hMod, "SK_GetVersionStr") && tool_seen.insert("Special K (proxy)").second) {
                detected_tools.push_back({ "Special K (proxy)", "injector" });
                PRINT_DEBUG("detected tool [injector]: Special K (proxy via %ls)", dll_name);
            } else if (GetProcAddress(hMod, "ReShadeVersion") && tool_seen.insert("ReShade (proxy)").second) {
                detected_tools.push_back({ "ReShade (proxy)", "post-processor" });
                PRINT_DEBUG("detected tool [post-processor]: ReShade (proxy via %ls)", dll_name);
            } else if (GetProcAddress(hMod, "GetASILoadLibrary") && tool_seen.insert("Ultimate ASI Loader (proxy)").second) {
                detected_tools.push_back({ "Ultimate ASI Loader (proxy)", "modding" });
                PRINT_DEBUG("detected tool [modding]: Ultimate ASI Loader (proxy via %ls)", dll_name);
            }
        }
    }

    // build replacement strings for the process tree placeholder (one per line ending style)
    // helper to format one entry
    auto format_entry = [](const DetectedRenderer& d, const std::string& indent, const std::string& eol) -> std::string {
        std::string r = indent + "[" + d.category + "] " + d.label;
        if (d.is_proxy) r += " [" + d.proxy_label + "]";
        if (!d.annotation.empty()) r += " (" + d.annotation + ")";
        r += eol;
        r += indent + "  " + d.full_path + eol;
        return r;
    };

    std::string replacement_lf, replacement_crlf;
    if (detected.empty()) {
        replacement_lf = "  Renderers: (none detected)\n";
        replacement_crlf = "  Renderers: (none detected)\r\n";
    } else if (detected.size() == 1) {
        replacement_lf = "  Renderers:\n" + format_entry(detected[0], "    ", "\n");
        replacement_crlf = "  Renderers:\r\n" + format_entry(detected[0], "    ", "\r\n");
    } else {
        replacement_lf = "  Renderers:\n";
        replacement_crlf = "  Renderers:\r\n";
        for (auto& d : detected) {
            replacement_lf += format_entry(d, "    ", "\n");
            replacement_crlf += format_entry(d, "    ", "\r\n");
        }
    }

    // read-modify-write to replace "(pending - detected after init)" in the process tree
    {
        FILE* rf = _wfopen(out_path.c_str(), L"rb");
        if (rf) {
            fseek(rf, 0, SEEK_END);
            long file_size = ftell(rf);
            fseek(rf, 0, SEEK_SET);
            if (file_size > 0) {
                std::string content(file_size, '\0');
                fread(&content[0], 1, file_size, rf);
                fclose(rf);

                const std::string placeholder_crlf = "  Renderers: (pending - detected after init)\r\n";
                const std::string placeholder_lf = "  Renderers: (pending - detected after init)\n";

                size_t pos = content.find(placeholder_crlf);
                if (pos != std::string::npos) {
                    content.replace(pos, placeholder_crlf.size(), replacement_crlf);
                } else {
                    pos = content.find(placeholder_lf);
                    if (pos != std::string::npos) {
                        content.replace(pos, placeholder_lf.size(), replacement_lf);
                    }
                }

                FILE* wf = _wfopen(out_path.c_str(), L"wb");
                if (wf) {
                    fwrite(content.c_str(), 1, content.size(), wf);
                    fclose(wf);
                }
            } else {
                fclose(rf);
            }
        }
    }

    // append detailed renderer & environment info
    FILE* f = _wfopen(out_path.c_str(), L"a");
    if (!f) return;

    fprintf(f, "=== Renderer & Environment Detection (PID %lu) ===\n", GetCurrentProcessId());

    // environment info
    if (!env_info.empty()) {
        fprintf(f, "  Environment:\n");
        for (auto& e : env_info) {
            fprintf(f, "    %s\n", e.c_str());
            PRINT_DEBUG("environment detected: %s", e.c_str());
        }
    }

    // translation layers
    if (!translation_info.empty()) {
        fprintf(f, "  Translation layers:\n");
        for (auto& t : translation_info) {
            fprintf(f, "    %s\n", t.c_str());
            PRINT_DEBUG("translation layer detected: %s", t.c_str());
        }
    }

    // Vulkan layers
    if (!vk_layer_info.empty()) {
        fprintf(f, "  Vulkan layers:\n");
        for (auto& v : vk_layer_info) {
            fprintf(f, "    %s\n", v.c_str());
            PRINT_DEBUG("vulkan layer detected: %s", v.c_str());
        }
    }

    // renderers (categorized)
    fprintf(f, "  Detected modules:\n");
    if (detected.empty()) {
        fprintf(f, "    (none detected)\n");
    } else {
        for (auto& d : detected) {
            fprintf(f, "    [%s] %s", d.category.c_str(), d.label.c_str());
            if (d.is_proxy) fprintf(f, " [PROXY: %s]", d.proxy_label.c_str());
            if (!d.annotation.empty()) fprintf(f, " (%s)", d.annotation.c_str());
            fprintf(f, "\n");
            fprintf(f, "      %s\n", d.full_path.c_str());
            PRINT_DEBUG("detected [%s]: %s at %s", d.category.c_str(), d.label.c_str(), d.full_path.c_str());
        }
    }

    // third-party tools (non-renderer)
    if (!detected_tools.empty()) {
        fprintf(f, "  Third-party tools:\n");
        for (auto& t : detected_tools) {
            fprintf(f, "    [%s] %s\n", t.type.c_str(), t.label.c_str());
            PRINT_DEBUG("tool detected: [%s] %s", t.type.c_str(), t.label.c_str());
        }
    }

    fprintf(f, "\n");

    fclose(f);
}

BOOL WINAPI DllMain( HINSTANCE, DWORD dwReason, LPVOID )
{
    switch ( dwReason ) {
        case DLL_PROCESS_ATTACH:
            PRINT_DEBUG("experimental DLL_PROCESS_ATTACH");
            dump_process_tree();
            if (!settings_disable_lan_only()) {
                PRINT_DEBUG("Hooking lan only functions");
                DetourTransactionBegin();
                DetourUpdateThread( GetCurrentThread() );
                DetourAttach( reinterpret_cast<PVOID*>(&Real_SendTo), reinterpret_cast<PVOID>(Mine_SendTo) );
                DetourAttach( reinterpret_cast<PVOID*>(&Real_Connect), reinterpret_cast<PVOID>(Mine_Connect) );
                DetourAttach( reinterpret_cast<PVOID*>(&Real_WSAConnect), reinterpret_cast<PVOID>(Mine_WSAConnect) );

                HMODULE winhttp = GetModuleHandleA("winhttp.dll");
                if (winhttp) {
                    Real_WinHttpConnect = (decltype(Real_WinHttpConnect))GetProcAddress(winhttp, "WinHttpConnect");
                    DetourAttach( reinterpret_cast<PVOID*>(&Real_WinHttpConnect), reinterpret_cast<PVOID>(Mine_WinHttpConnect) );
                    // Real_WinHttpOpenRequest = (decltype(Real_WinHttpOpenRequest))GetProcAddress(winhttp, "WinHttpOpenRequest");
                    // DetourAttach( reinterpret_cast<PVOID*>(&Real_WinHttpOpenRequest), reinterpret_cast<PVOID>(Mine_WinHttpOpenRequest) );
                }
    
                DetourTransactionCommit();
                network_functions_attached = true;
            }
            load_crack_dll();
            load_dlls();
        break;

        case DLL_PROCESS_DETACH:
            PRINT_DEBUG("experimental DLL_PROCESS_DETACH");
            if (network_functions_attached) {
                DetourTransactionBegin();
                DetourUpdateThread( GetCurrentThread() );
                DetourDetach( reinterpret_cast<PVOID*>(&Real_SendTo), reinterpret_cast<PVOID>(Mine_SendTo) );
                DetourDetach( reinterpret_cast<PVOID*>(&Real_Connect), reinterpret_cast<PVOID>(Mine_Connect) );
                DetourDetach( reinterpret_cast<PVOID*>(&Real_WSAConnect), reinterpret_cast<PVOID>(Mine_WSAConnect) );
                if (Real_WinHttpConnect) {
                    DetourDetach( reinterpret_cast<PVOID*>(&Real_WinHttpConnect), reinterpret_cast<PVOID>(Mine_WinHttpConnect) );
                    // DetourDetach( reinterpret_cast<PVOID*>(&Real_WinHttpOpenRequest), reinterpret_cast<PVOID>(Mine_WinHttpOpenRequest) );
                }
                DetourTransactionCommit();
            }

            unload_dlls();
        break;
    }

    return TRUE;
}

#else

// dump the full process tree of the current process to <so_name>.txt
// uses /proc filesystem for process info, command lines, and parent traversal
static void dump_process_tree()
{
    // get our .so path via dladdr
    static const char anchor = 0;
    Dl_info dl_info{};
    if (!dladdr((void*)&anchor, &dl_info) || !dl_info.dli_fname) return;

    std::string so_path(dl_info.dli_fname);
    // resolve to absolute path
    char resolved[PATH_MAX]{};
    if (realpath(so_path.c_str(), resolved)) so_path = resolved;

    // derive output file path: <so_name>.txt next to the .so
    std::string out_path = so_path + ".txt";

    // derive directory for relative path computation
    std::string so_dir = so_path;
    auto last_sep = so_dir.find_last_of('/');
    if (last_sep != std::string::npos) so_dir.resize(last_sep + 1);

    // get .so filename for header
    std::string so_name = (last_sep != std::string::npos) ? so_path.substr(last_sep + 1) : so_path;

    struct ProcessInfo {
        pid_t pid;
        pid_t parent_pid;
        std::string exe_name;
        std::string full_path;
        std::string cmdline;
        std::string start_time; // formatted creation timestamp
        struct RendererEntry {
            std::string label;
            std::string full_path;
            bool is_proxy;
        };
        std::vector<RendererEntry> renderers; // loaded renderer/overlay libs (for parent processes)
    };
    std::vector<ProcessInfo> tree;

    // libraries to look for when scanning parent process maps
    struct { const char* lib; const char* label; } renderer_libs[] = {
        { "libvulkan.so",     "Vulkan" },
        { "libGL.so",         "OpenGL" },
        { "libGLX.so",        "GLX" },
        { "libEGL.so",        "EGL" },
        { "libGLESv2.so",     "OpenGL ES" },
        { "libvkbasalt.so",   "vkBasalt" },
        // DXVK-native (D3D -> Vulkan for native Linux games)
        { "libdxvk_d3d9.so",  "DXVK-native (D3D9)" },
        { "libdxvk_d3d11.so", "DXVK-native (D3D11)" },
        { "libdxvk_dxgi.so",  "DXVK-native (DXGI)" },
        { "SpecialK",         "Special K" },
        { "ReShade",          "ReShade" },
    };

    // standard system library paths (libraries here are NOT proxies)
    const char* parent_system_prefixes[] = {
        "/usr/lib", "/usr/lib32", "/usr/lib64",
        "/usr/lib/x86_64-linux-gnu", "/usr/lib/i386-linux-gnu",
        "/lib/", "/lib64/", "/lib32/",
        "/usr/local/lib",
        "/nix/store",
    };

    // walk from current process up through parents
    pid_t walk_pid = getpid();
    std::set<pid_t> visited;
    while (walk_pid > 0 && visited.insert(walk_pid).second) {
        ProcessInfo info{};
        info.pid = walk_pid;

        // read exe path from /proc/<pid>/exe
        char link_path[64]{};
        snprintf(link_path, sizeof(link_path), "/proc/%d/exe", walk_pid);
        char exe_buf[PATH_MAX]{};
        ssize_t len = readlink(link_path, exe_buf, sizeof(exe_buf) - 1);
        if (len > 0) {
            exe_buf[len] = '\0';
            info.full_path = exe_buf;
            auto slash = info.full_path.find_last_of('/');
            info.exe_name = (slash != std::string::npos) ? info.full_path.substr(slash + 1) : info.full_path;
        }

        // read command line from /proc/<pid>/cmdline (NUL-separated args)
        char cmd_path[64]{};
        snprintf(cmd_path, sizeof(cmd_path), "/proc/%d/cmdline", walk_pid);
        FILE* cf = fopen(cmd_path, "r");
        if (cf) {
            char cmd_buf[4096]{};
            size_t n = fread(cmd_buf, 1, sizeof(cmd_buf) - 1, cf);
            fclose(cf);
            // replace NUL separators with spaces
            for (size_t i = 0; i < n; ++i) {
                if (cmd_buf[i] == '\0') cmd_buf[i] = ' ';
            }
            if (n > 0 && cmd_buf[n - 1] == ' ') cmd_buf[n - 1] = '\0';
            info.cmdline = cmd_buf;
        }

        // read parent PID and start time from /proc/<pid>/stat
        info.parent_pid = 0;
        char stat_path[64]{};
        snprintf(stat_path, sizeof(stat_path), "/proc/%d/stat", walk_pid);
        FILE* sf = fopen(stat_path, "r");
        if (sf) {
            char stat_buf[4096]{};
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wunused-result"
            (void)fread(stat_buf, 1, sizeof(stat_buf) - 1, sf);
#pragma GCC diagnostic pop
            fclose(sf);
            // format: pid (comm) state ppid ... field22=starttime
            // find the last ')' to skip comm which may contain spaces/parens
            char* comm_end = strrchr(stat_buf, ')');
            if (comm_end) {
                int ppid = 0;
                char state;
                if (sscanf(comm_end + 1, " %c %d", &state, &ppid) == 2) {
                    info.parent_pid = ppid;
                }
                // parse starttime (field 22, which is field 20 after comm_end)
                // fields after ')': state(1) ppid(2) pgrp(3) session(4) tty_nr(5) tpgid(6)
                //   flags(7) minflt(8) cminflt(9) majflt(10) cmajflt(11) utime(12) stime(13)
                //   cutime(14) cstime(15) priority(16) nice(17) num_threads(18) itrealvalue(19)
                //   starttime(20)
                unsigned long long starttime = 0;
                char* p = comm_end + 2; // skip ') '
                int field = 0;
                while (*p && field < 19) {
                    while (*p == ' ') ++p;
                    if (!*p) break;
                    if (field == 19) break;
                    while (*p && *p != ' ') ++p;
                    ++field;
                }
                while (*p == ' ') ++p;
                if (*p) {
                    sscanf(p, "%llu", &starttime);
                    if (starttime > 0) {
                        // convert clock ticks since boot to wall clock time
                        long hz = sysconf(_SC_CLK_TCK);
                        if (hz > 0) {
                            // read boot time from /proc/stat
                            FILE* bf = fopen("/proc/stat", "r");
                            unsigned long long btime = 0;
                            if (bf) {
                                char line[256]{};
                                while (fgets(line, sizeof(line), bf)) {
                                    if (strncmp(line, "btime ", 6) == 0) {
                                        sscanf(line + 6, "%llu", &btime);
                                        break;
                                    }
                                }
                                fclose(bf);
                            }
                            if (btime > 0) {
                                time_t proc_start = (time_t)(btime + starttime / hz);
                                struct tm tm_start{};
                                localtime_r(&proc_start, &tm_start);
                                char tbuf[64]{};
                                snprintf(tbuf, sizeof(tbuf), "%04d-%02d-%02d %02d:%02d:%02d",
                                    tm_start.tm_year + 1900, tm_start.tm_mon + 1, tm_start.tm_mday,
                                    tm_start.tm_hour, tm_start.tm_min, tm_start.tm_sec);
                                info.start_time = tbuf;
                            }
                        }
                    }
                }
            }
        }

        // scan /proc/<pid>/maps for renderer/overlay libraries (skip current process - renderer not loaded yet)
        if (walk_pid != getpid()) {
            char maps_path[64]{};
            snprintf(maps_path, sizeof(maps_path), "/proc/%d/maps", walk_pid);
            FILE* mf = fopen(maps_path, "r");
            if (mf) {
                char mline[1024]{};
                std::set<std::string> rseen;
                while (fgets(mline, sizeof(mline), mf)) {
                    for (auto& rl : renderer_libs) {
                        if (strstr(mline, rl.lib) && rseen.insert(rl.label).second) {
                            ProcessInfo::RendererEntry entry{};
                            entry.label = rl.label;
                            // extract full mapped path from maps line
                            const char* p = mline;
                            int fields = 0;
                            while (*p && fields < 5) {
                                while (*p == ' ') ++p;
                                while (*p && *p != ' ') ++p;
                                ++fields;
                            }
                            while (*p == ' ') ++p;
                            if (*p == '/') {
                                entry.full_path = p;
                                while (!entry.full_path.empty() && (entry.full_path.back() == '\n' || entry.full_path.back() == '\r'))
                                    entry.full_path.pop_back();
                                // proxy detection: check against system prefixes
                                bool in_system = false;
                                for (auto& prefix : parent_system_prefixes) {
                                    if (strncmp(entry.full_path.c_str(), prefix, strlen(prefix)) == 0) {
                                        in_system = true;
                                        break;
                                    }
                                }
                                entry.is_proxy = !in_system;
                            }
                            info.renderers.push_back(std::move(entry));
                        }
                    }
                }
                fclose(mf);
            }
        }

        tree.push_back(std::move(info));
        walk_pid = tree.back().parent_pid;
    }

    // compute relative path from .so directory
    auto make_relative = [&so_dir](const std::string& full_path) -> std::string {
        if (full_path.empty()) return "(unknown)";
        if (full_path.size() >= so_dir.size() &&
            strncmp(full_path.c_str(), so_dir.c_str(), so_dir.size()) == 0) {
            return "./" + full_path.substr(so_dir.size());
        }
        return full_path;
    };

    // sanitize home directory
    const char* home = getenv("HOME");
    size_t home_len = home ? strlen(home) : 0;
    auto sanitize = [home, home_len](const std::string& path) -> std::string {
        if (!home || home_len == 0 || path.size() < home_len) return path;
        if (strncmp(path.c_str(), home, home_len) == 0) {
            return "$HOME" + path.substr(home_len);
        }
        return path;
    };

    // write the file
    FILE* f = fopen(out_path.c_str(), "w");
    if (!f) return;

    // timestamp
    time_t now = time(nullptr);
    struct tm tm_buf{};
    localtime_r(&now, &tm_buf);
    fprintf(f, "=== %s Process Tree ===\n", so_name.c_str());
    fprintf(f, "Timestamp: %04d-%02d-%02d %02d:%02d:%02d\n",
        tm_buf.tm_year + 1900, tm_buf.tm_mon + 1, tm_buf.tm_mday,
        tm_buf.tm_hour, tm_buf.tm_min, tm_buf.tm_sec);
    fprintf(f, "SO Path: %s\n\n", sanitize(so_path).c_str());

    pid_t current_pid = getpid();
    for (size_t i = 0; i < tree.size(); ++i) {
        auto& p = tree[i];
        bool is_current = (p.pid == current_pid);

        fprintf(f, "[PID %d] %s%s\n", p.pid, p.exe_name.c_str(), is_current ? "  (current process)" : "");
        fprintf(f, "  Path: %s\n", sanitize(p.full_path).c_str());
        fprintf(f, "  Relative: %s\n", sanitize(make_relative(p.full_path)).c_str());
        if (!p.start_time.empty()) {
            fprintf(f, "  Started: %s\n", p.start_time.c_str());
        }
        if (!p.cmdline.empty()) {
            fprintf(f, "  CmdLine: %s\n", sanitize(p.cmdline).c_str());
        }
        if (is_current) {
            fprintf(f, "  Renderers: (pending - detected after init)\n");
        } else if (!p.renderers.empty()) {
            if (p.renderers.size() == 1) {
                auto& r = p.renderers[0];
                if (r.is_proxy) {
                    fprintf(f, "  Renderers: %s [PROXY]\n", r.label.c_str());
                } else {
                    fprintf(f, "  Renderers: %s\n", r.label.c_str());
                }
                if (!r.full_path.empty()) {
                    fprintf(f, "    %s\n", sanitize(r.full_path).c_str());
                }
            } else {
                fprintf(f, "  Renderers:\n");
                for (auto& r : p.renderers) {
                    if (r.is_proxy) {
                        fprintf(f, "    %s [PROXY]\n", r.label.c_str());
                    } else {
                        fprintf(f, "    %s\n", r.label.c_str());
                    }
                    if (!r.full_path.empty()) {
                        fprintf(f, "      %s\n", sanitize(r.full_path).c_str());
                    }
                }
            }
        }
        fprintf(f, "\n");

        PRINT_DEBUG("process tree [PID %d]: %s%s | path: %s", p.pid, p.exe_name.c_str(),
            is_current ? " (current)" : "", sanitize(p.full_path).c_str());
    }

    fclose(f);
}

// append detected renderer(s) for the current process
// called from SteamAPI_RunCallbacks (first call only), when the game's renderer is initialized
void append_renderer_info()
{
    // get our .so path via dladdr
    static const char anchor = 0;
    Dl_info dl_info{};
    if (!dladdr((void*)&anchor, &dl_info) || !dl_info.dli_fname) return;

    std::string so_path(dl_info.dli_fname);
    char resolved[PATH_MAX]{};
    if (realpath(so_path.c_str(), resolved)) so_path = resolved;

    std::string out_path = so_path + ".txt";

    // for sanitizing home directory in paths
    const char* home = getenv("HOME");
    size_t home_len = home ? strlen(home) : 0;

    // --- Environment detection ---
    std::vector<std::string> env_info;

    // display server
    const char* wayland = getenv("WAYLAND_DISPLAY");
    const char* x_display = getenv("DISPLAY");
    const char* session_type = getenv("XDG_SESSION_TYPE");
    if (session_type) {
        std::string sess = "Session: ";
        sess += session_type;
        if (wayland) { sess += " ("; sess += wayland; sess += ")"; }
        else if (x_display) { sess += " ("; sess += x_display; sess += ")"; }
        env_info.push_back(sess);
    } else {
        if (wayland) env_info.push_back(std::string("Wayland (") + wayland + ")");
        else if (x_display) env_info.push_back(std::string("X11 (") + x_display + ")");
    }

    // Steam runtime / launched from Steam
    const char* steam_runtime = getenv("STEAM_RUNTIME");
    if (steam_runtime) env_info.push_back(std::string("Steam Runtime: ") + steam_runtime);
    const char* steam_appid = getenv("SteamAppId");
    if (steam_appid) env_info.push_back(std::string("SteamAppId: ") + steam_appid);

    // Proton (when running native .so side-by-side with Proton game)
    const char* compat_data = getenv("STEAM_COMPAT_DATA_PATH");
    if (compat_data) {
        std::string label = "Proton (Steam Play)";
        const char* proton_ver = getenv("PROTON_VERSION");
        if (proton_ver) { label += " "; label += proton_ver; }
        env_info.push_back(label);
    }

    // Lutris
    if (getenv("LUTRIS_GAME_SLUG"))
        env_info.push_back("Lutris");

    // Bottles
    const char* flatpak_id = getenv("FLATPAK_ID");
    if (getenv("BOTTLES_ENV") || (flatpak_id && strstr(flatpak_id, "bottles")))
        env_info.push_back("Bottles");

    // PlayOnLinux
    if (getenv("PLAYONLINUX") || getenv("POL_WINEVERSION"))
        env_info.push_back("PlayOnLinux");

    // CrossOver
    if (getenv("CX_BOTTLE") || getenv("CX_ROOT"))
        env_info.push_back("CrossOver");

    // Heroic Games Launcher
    const char* store_env = getenv("STORE");
    if (getenv("HEROIC_APP_NAME") || (store_env && (strstr(store_env, "legendary") || strstr(store_env, "gog"))))
        env_info.push_back("Heroic Games Launcher");

    // GameScope
    if (getenv("GAMESCOPE_WAYLAND_DISPLAY"))
        env_info.push_back("GameScope");

    // MangoHud
    if (getenv("MANGOHUD"))
        env_info.push_back("MangoHud");

    // Flatpak / Snap
    if (flatpak_id) env_info.push_back(std::string("Flatpak: ") + flatpak_id);
    if (getenv("SNAP")) env_info.push_back("Snap");

    // --- Mesa / GPU driver detection ---
    std::vector<std::string> driver_info;
    const char* mesa_driver = getenv("MESA_LOADER_DRIVER_OVERRIDE");
    if (mesa_driver) {
        std::string label = "Mesa driver override: ";
        label += mesa_driver;
        if (strstr(mesa_driver, "zink"))
            label += " (OpenGL -> Vulkan)";
        driver_info.push_back(label);
    }
    const char* libva_driver = getenv("LIBVA_DRIVER_NAME");
    if (libva_driver) driver_info.push_back(std::string("VA-API driver: ") + libva_driver);
    const char* vdpau_driver = getenv("VDPAU_DRIVER");
    if (vdpau_driver) driver_info.push_back(std::string("VDPAU driver: ") + vdpau_driver);

    // --- Vulkan layer detection ---
    std::vector<std::string> vk_layer_info;

    // check VK_INSTANCE_LAYERS env var
    const char* vk_layers = getenv("VK_INSTANCE_LAYERS");
    if (vk_layers) {
        if (strstr(vk_layers, "VK_LAYER_reshade"))
            vk_layer_info.push_back("ReShade (Vulkan layer via VK_INSTANCE_LAYERS)");
        if (strstr(vk_layers, "VK_LAYER_vkBasalt") || strstr(vk_layers, "vkBasalt"))
            vk_layer_info.push_back("vkBasalt (Vulkan layer via VK_INSTANCE_LAYERS)");
        if (strstr(vk_layers, "VK_LAYER_MANGOHUD") || strstr(vk_layers, "MangoHud"))
            vk_layer_info.push_back("MangoHud (Vulkan layer via VK_INSTANCE_LAYERS)");
    }

    // check ENABLE_VKBASALT env var
    if (getenv("ENABLE_VKBASALT"))
        vk_layer_info.push_back("vkBasalt (enabled via ENABLE_VKBASALT)");

    // scan for installed Vulkan layer manifests
    const char* vk_layer_dirs[] = {
        "/usr/share/vulkan/implicit_layer.d",
        "/usr/share/vulkan/explicit_layer.d",
        "/etc/vulkan/implicit_layer.d",
        "/etc/vulkan/explicit_layer.d",
    };
    // also check XDG_DATA_HOME and HOME for user-installed layers
    std::vector<std::string> user_layer_dirs;
    const char* xdg_data = getenv("XDG_DATA_HOME");
    if (xdg_data) {
        user_layer_dirs.push_back(std::string(xdg_data) + "/vulkan/implicit_layer.d");
        user_layer_dirs.push_back(std::string(xdg_data) + "/vulkan/explicit_layer.d");
    } else if (home && home_len > 0) {
        user_layer_dirs.push_back(std::string(home) + "/.local/share/vulkan/implicit_layer.d");
        user_layer_dirs.push_back(std::string(home) + "/.local/share/vulkan/explicit_layer.d");
    }

    auto scan_layer_dir = [&vk_layer_info](const char* dir_path, bool is_user) {
        DIR* dir = opendir(dir_path);
        if (!dir) return;
        struct dirent* entry;
        while ((entry = readdir(dir)) != nullptr) {
            if (!entry->d_name || entry->d_name[0] == '.') continue;
            const char* name = entry->d_name;
            bool is_implicit = (strstr(dir_path, "implicit") != nullptr);
            if (strstr(name, "reshade") || strstr(name, "ReShade")) {
                vk_layer_info.push_back(std::string("ReShade (Vulkan ") +
                    (is_implicit ? "implicit" : "explicit") + " layer: " +
                    dir_path + "/" + name + (is_user ? " [user]" : "") + ")");
            }
            if (strstr(name, "vkbasalt") || strstr(name, "vkBasalt")) {
                vk_layer_info.push_back(std::string("vkBasalt (Vulkan ") +
                    (is_implicit ? "implicit" : "explicit") + " layer: " +
                    dir_path + "/" + name + (is_user ? " [user]" : "") + ")");
            }
            if (strstr(name, "mangohud") || strstr(name, "MangoHud")) {
                vk_layer_info.push_back(std::string("MangoHud (Vulkan ") +
                    (is_implicit ? "implicit" : "explicit") + " layer: " +
                    dir_path + "/" + name + (is_user ? " [user]" : "") + ")");
            }
        }
        closedir(dir);
    };

    for (auto& ld : vk_layer_dirs) {
        scan_layer_dir(ld, false);
    }
    for (auto& uld : user_layer_dirs) {
        scan_layer_dir(uld.c_str(), true);
    }

    // check /proc/self/maps for renderer and overlay libraries
    struct { const char* lib; const char* label; } renderers[] = {
        { "libvulkan.so",     "Vulkan" },
        { "libGL.so",         "OpenGL" },
        { "libGLX.so",        "GLX" },
        { "libEGL.so",        "EGL" },
        { "libGLESv1_CM.so",  "OpenGL ES 1.x" },
        { "libGLESv2.so",     "OpenGL ES 2/3" },
        { "libSDL2",          "SDL2" },
        { "libSDL3",          "SDL3" },
        { "libwayland-client", "Wayland client" },
        { "libX11.so",        "X11 client" },
        { "libvkbasalt.so",   "vkBasalt" },
        // DXVK-native (D3D -> Vulkan for native Linux games)
        { "libdxvk_d3d9.so",  "DXVK-native (D3D9)" },
        { "libdxvk_d3d11.so", "DXVK-native (D3D11)" },
        { "libdxvk_dxgi.so",  "DXVK-native (DXGI)" },
        { "SpecialK",         "Special K" },
        { "ReShade",          "ReShade" },
    };

    // standard system library paths (libraries here are NOT proxies)
    const char* system_prefixes[] = {
        "/usr/lib", "/usr/lib32", "/usr/lib64",
        "/usr/lib/x86_64-linux-gnu", "/usr/lib/i386-linux-gnu",
        "/lib/", "/lib64/", "/lib32/",
        "/usr/local/lib",
        "/nix/store",
    };

    struct DetectedRenderer {
        std::string label;
        std::string full_path;  // full mapped path
        std::string filename;   // just the filename
        bool is_proxy;
        std::string category;    // GAME, OVERLAY, TRANSLATION, INFRA
        std::string annotation;
    };
    std::vector<DetectedRenderer> detected;
    FILE* maps = fopen("/proc/self/maps", "r");
    if (maps) {
        char line[1024]{};
        std::set<std::string> seen;
        while (fgets(line, sizeof(line), maps)) {
            for (auto& r : renderers) {
                if (strstr(line, r.lib) && seen.insert(r.label).second) {
                    DetectedRenderer entry{};
                    entry.label = r.label;

                    // extract the full mapped file path from the line
                    // maps format: addr perms offset dev inode  pathname
                    // find the pathname (starts after inode, leading spaces trimmed)
                    const char* p = line;
                    int fields = 0;
                    while (*p && fields < 5) {
                        while (*p == ' ') ++p;
                        while (*p && *p != ' ') ++p;
                        ++fields;
                    }
                    while (*p == ' ') ++p;
                    if (*p && *p == '/') {
                        entry.full_path = p;
                        // trim trailing newline
                        while (!entry.full_path.empty() && (entry.full_path.back() == '\n' || entry.full_path.back() == '\r'))
                            entry.full_path.pop_back();

                        // extract just the filename
                        auto slash = entry.full_path.find_last_of('/');
                        entry.filename = (slash != std::string::npos) ? entry.full_path.substr(slash + 1) : entry.full_path;

                        // check if it's in a standard system path
                        bool in_system = false;
                        for (auto& prefix : system_prefixes) {
                            if (strncmp(entry.full_path.c_str(), prefix, strlen(prefix)) == 0) {
                                in_system = true;
                                break;
                            }
                        }
                        entry.is_proxy = !in_system;
                    } else {
                        entry.filename = r.lib;
                    }

                    // sanitize home dir in full path
                    if (!entry.full_path.empty() && home && home_len > 0 &&
                        strncmp(entry.full_path.c_str(), home, home_len) == 0) {
                        entry.full_path = "$HOME" + entry.full_path.substr(home_len);
                    }

                    detected.push_back(std::move(entry));
                }
            }
        }
        fclose(maps);
    }

    // --- classify each detected entry ---
    for (auto& d : detected) {
        // overlay tools
        if (d.label == "vkBasalt") {
            d.category = "OVERLAY";
            d.annotation = "Vulkan post-processing layer";
            continue;
        }
        if (d.label == "Special K") {
            d.category = "OVERLAY";
            continue;
        }
        if (d.label == "ReShade") {
            d.category = "OVERLAY";
            continue;
        }

        // DXVK-native translation layers
        if (d.label.find("DXVK-native") != std::string::npos) {
            d.category = "TRANSLATION";
            d.annotation = "translates " + d.label + " -> Vulkan";
            continue;
        }

        // infrastructure (windowing / framework, not direct renderers)
        if (d.label == "Wayland client" || d.label == "X11 client") {
            d.category = "INFRA";
            continue;
        }
        if (d.label == "SDL2" || d.label == "SDL3") {
            d.category = "INFRA";
            continue;
        }
        if (d.label == "GLX" || d.label == "EGL") {
            d.category = "INFRA";
            continue;
        }

        // everything else = game renderer
        d.category = "GAME";
    }

    // --- Third-party tool detection (non-renderer modules via /proc/self/maps) ---
    struct DetectedTool {
        std::string label;
        std::string type;
        std::string path;
    };
    std::vector<DetectedTool> detected_tools;
    {
        struct { const char* lib; const char* label; const char* type; } known_tools[] = {
            // --- recording / streaming ---
            { "libobs.so",                  "OBS Studio",           "recording" },
            { "libobs-opengl.so",           "OBS OpenGL Capture",   "recording" },
            { "libobs-vulkan.so",           "OBS Vulkan Capture",   "recording" },
            { "gpu-screen-recorder",        "GPU Screen Recorder",  "recording" },

            // --- monitoring ---
            { "libMangoHud.so",             "MangoHud",             "monitoring" },
            { "libMangoHud_dlsym.so",       "MangoHud",             "monitoring" },

            // --- post-processing / overlay ---
            { "libvkbasalt.so",             "vkBasalt",             "post-processor" },
            { "libreshade.so",              "ReShade",              "post-processor" },

            // --- store overlays ---
            { "gameoverlayrenderer.so",     "Steam Overlay",        "store overlay" },
            { "discord_game_sdk.so",        "Discord Game SDK",     "store overlay" },

            // --- gaming platforms ---
            { "libgamemodeauto.so",         "GameMode (Feral)",     "platform" },
            { "libgamemode.so",             "GameMode (Feral)",     "platform" },

            // --- frame limiting ---
            { "libstrangle.so",             "libstrangle",          "frame limiter" },

            // --- communication ---
            { "libmumble.so",               "Mumble",               "communication" },
            { "mumble_ol.so",               "Mumble",               "communication" },

            // --- VR ---
            { "libopenvr_api.so",           "SteamVR",              "vr" },
            { "libopenxr_loader.so",        "OpenXR",               "vr" },

            // --- anti-cheat (informational) ---
            { "libeasyanticheat.so",        "EasyAntiCheat",        "anti-cheat" },
            { "easyanticheat_x64.so",       "EasyAntiCheat",        "anti-cheat" },
            { "easyanticheat_x86.so",       "EasyAntiCheat",        "anti-cheat" },
            { "battleye_client.so",         "BattlEye",             "anti-cheat" },
            { "beclient_x64.so",            "BattlEye",             "anti-cheat" },
        };
        FILE* tool_maps = fopen("/proc/self/maps", "r");
        if (tool_maps) {
            char tline[1024]{};
            std::set<std::string> tool_seen;
            while (fgets(tline, sizeof(tline), tool_maps)) {
                for (auto& t : known_tools) {
                    if (strstr(tline, t.lib) && tool_seen.insert(t.label).second) {
                        DetectedTool tool_entry{};
                        tool_entry.label = t.label;
                        tool_entry.type = t.type;
                        // extract path from maps line
                        const char* p = tline;
                        int fields = 0;
                        while (*p && fields < 5) {
                            while (*p == ' ') ++p;
                            while (*p && *p != ' ') ++p;
                            ++fields;
                        }
                        while (*p == ' ') ++p;
                        if (*p == '/') {
                            tool_entry.path = p;
                            while (!tool_entry.path.empty() && (tool_entry.path.back() == '\n' || tool_entry.path.back() == '\r'))
                                tool_entry.path.pop_back();
                            if (home && home_len > 0 && strncmp(tool_entry.path.c_str(), home, home_len) == 0) {
                                tool_entry.path = "$HOME" + tool_entry.path.substr(home_len);
                            }
                        }
                        detected_tools.push_back(std::move(tool_entry));
                        PRINT_DEBUG("detected tool [%s]: %s", t.type, t.label);
                    }
                }
            }
            fclose(tool_maps);
        }
    }

    // build replacement string for the process tree placeholder
    auto format_entry_linux = [](const DetectedRenderer& d, const std::string& indent) -> std::string {
        std::string r = indent + "[" + d.category + "] " + d.label;
        if (d.is_proxy) r += " [PROXY]";
        if (!d.annotation.empty()) r += " (" + d.annotation + ")";
        r += "\n";
        if (!d.full_path.empty()) {
            r += indent + "  " + d.full_path + "\n";
        }
        return r;
    };

    std::string replacement;
    if (detected.empty()) {
        replacement = "  Renderers: (none detected)\n";
    } else if (detected.size() == 1) {
        replacement = "  Renderers:\n" + format_entry_linux(detected[0], "    ");
    } else {
        replacement = "  Renderers:\n";
        for (auto& d : detected) {
            replacement += format_entry_linux(d, "    ");
        }
    }

    // read-modify-write to replace "(pending - detected after init)" in the process tree
    {
        FILE* rf = fopen(out_path.c_str(), "rb");
        if (rf) {
            fseek(rf, 0, SEEK_END);
            long file_size = ftell(rf);
            fseek(rf, 0, SEEK_SET);
            if (file_size > 0) {
                std::string content(file_size, '\0');
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wunused-result"
                (void)fread(&content[0], 1, file_size, rf);
#pragma GCC diagnostic pop
                fclose(rf);

                const std::string placeholder = "  Renderers: (pending - detected after init)\n";

                size_t pos = content.find(placeholder);
                if (pos != std::string::npos) {
                    content.replace(pos, placeholder.size(), replacement);
                }

                FILE* wf = fopen(out_path.c_str(), "wb");
                if (wf) {
                    fwrite(content.c_str(), 1, content.size(), wf);
                    fclose(wf);
                }
            } else {
                fclose(rf);
            }
        }
    }

    // append detailed renderer & environment info
    FILE* f = fopen(out_path.c_str(), "a");
    if (!f) return;

    fprintf(f, "=== Renderer & Environment Detection (PID %d) ===\n", getpid());

    // environment info
    if (!env_info.empty()) {
        fprintf(f, "  Environment:\n");
        for (auto& e : env_info) {
            fprintf(f, "    %s\n", e.c_str());
            PRINT_DEBUG("environment detected: %s", e.c_str());
        }
    }

    // driver info
    if (!driver_info.empty()) {
        fprintf(f, "  GPU/Driver:\n");
        for (auto& d : driver_info) {
            fprintf(f, "    %s\n", d.c_str());
            PRINT_DEBUG("driver info: %s", d.c_str());
        }
    }

    // Vulkan layers
    if (!vk_layer_info.empty()) {
        fprintf(f, "  Vulkan layers:\n");
        for (auto& v : vk_layer_info) {
            fprintf(f, "    %s\n", v.c_str());
            PRINT_DEBUG("vulkan layer detected: %s", v.c_str());
        }
    }

    // renderers
    fprintf(f, "  Detected modules:\n");
    if (detected.empty()) {
        fprintf(f, "    (none detected)\n");
    } else {
        for (auto& d : detected) {
            if (d.is_proxy) {
                fprintf(f, "    [%s] %s [PROXY: custom library]", d.category.c_str(), d.label.c_str());
            } else {
                fprintf(f, "    [%s] %s", d.category.c_str(), d.label.c_str());
            }
            if (!d.annotation.empty()) {
                fprintf(f, " (%s)", d.annotation.c_str());
            }
            fprintf(f, "\n");
            fprintf(f, "      %s\n", d.full_path.c_str());
            PRINT_DEBUG("module detected: [%s] %s at %s", d.category.c_str(), d.label.c_str(), d.full_path.c_str());
        }
    }

    // third-party tools (non-renderer)
    if (!detected_tools.empty()) {
        fprintf(f, "  Third-party tools:\n");
        for (auto& t : detected_tools) {
            fprintf(f, "    [%s] %s\n", t.type.c_str(), t.label.c_str());
            if (!t.path.empty()) {
                fprintf(f, "      %s\n", t.path.c_str());
            }
            PRINT_DEBUG("tool detected: [%s] %s", t.type.c_str(), t.label.c_str());
        }
    }

    fprintf(f, "\n");

    fclose(f);
}


// this acts as both an entry and an exit points for the library
// avoid "__attribute__((__constructor__))" and "__attribute__((__destructor__))"
// since they run before the C+++ runtime has been initialized
// causing problems related to uninitialized global objects
struct CppRuntimeTrick {
    CppRuntimeTrick()
    {
        PRINT_DEBUG_ENTRY();
        dump_process_tree();
        load_dlls();
    }

    ~CppRuntimeTrick()
    {
        PRINT_DEBUG_ENTRY();
        unload_dlls();
    }
} static g_cpp_rt{};


void set_whitelist_ips(uint32_t *from, uint32_t *to, unsigned num_ips)
{

}

#endif // __WINDOWS__

#else

void set_whitelist_ips(uint32_t *from, uint32_t *to, unsigned num_ips)
{

}

#endif // EMU_EXPERIMENTAL_BUILD
