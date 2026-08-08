#include "sffcore/CorePaths.h"

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#endif

#include <system_error>

namespace SffCore::Paths {
namespace {
std::filesystem::path ResolveModuleDirectory() {
#ifdef _WIN32
    HMODULE module = nullptr;
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                            reinterpret_cast<LPCWSTR>(&ResolveModuleDirectory), &module)) {
        return std::filesystem::current_path();
    }
    std::wstring buffer(32768, L'\0');
    DWORD length = GetModuleFileNameW(module, buffer.data(), static_cast<DWORD>(buffer.size()));
    if (length == 0 || length >= buffer.size()) return std::filesystem::current_path();
    buffer.resize(length);
    return std::filesystem::path(buffer).parent_path();
#else
    return std::filesystem::current_path();
#endif
}

const std::filesystem::path g_moduleDir = ResolveModuleDirectory();
const std::filesystem::path g_dataDir = g_moduleDir;
const std::filesystem::path g_manifestDir = [] {
    auto p = g_dataDir / "manifests";
    std::error_code ec;
    std::filesystem::create_directories(p, ec);
    return p;
}();
} // namespace

const std::filesystem::path& ModuleDirectory() { return g_moduleDir; }
const std::filesystem::path& DataDirectory() { return g_dataDir; }
const std::filesystem::path& ManifestStagingDirectory() { return g_manifestDir; }
} // namespace SffCore::Paths
