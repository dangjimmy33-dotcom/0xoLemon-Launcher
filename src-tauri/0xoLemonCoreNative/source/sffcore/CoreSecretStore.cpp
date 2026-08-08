#include "sffcore/CoreSecretStore.h"

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <dpapi.h>
#endif

#include <fstream>

namespace SffCore::SecretStore {
std::optional<std::vector<uint8_t>> ProtectString(std::string_view plaintext) {
#ifdef _WIN32
    DATA_BLOB in{};
    in.pbData = reinterpret_cast<BYTE*>(const_cast<char*>(plaintext.data()));
    in.cbData = static_cast<DWORD>(plaintext.size());
    DATA_BLOB out{};
    if (!CryptProtectData(&in, L"0xoLemonCore", nullptr, nullptr, nullptr, CRYPTPROTECT_UI_FORBIDDEN, &out))
        return std::nullopt;
    std::vector<uint8_t> result(out.pbData, out.pbData + out.cbData);
    LocalFree(out.pbData);
    return result;
#else
    (void)plaintext;
    return std::nullopt;
#endif
}

std::optional<std::string> UnprotectString(std::span<const uint8_t> ciphertext) {
#ifdef _WIN32
    if (ciphertext.empty()) return std::string{};
    DATA_BLOB in{};
    in.pbData = const_cast<BYTE*>(reinterpret_cast<const BYTE*>(ciphertext.data()));
    in.cbData = static_cast<DWORD>(ciphertext.size());
    DATA_BLOB out{};
    if (!CryptUnprotectData(&in, nullptr, nullptr, nullptr, nullptr, CRYPTPROTECT_UI_FORBIDDEN, &out))
        return std::nullopt;
    std::string result(reinterpret_cast<char*>(out.pbData), reinterpret_cast<char*>(out.pbData) + out.cbData);
    LocalFree(out.pbData);
    return result;
#else
    (void)ciphertext;
    return std::nullopt;
#endif
}

bool SaveProtectedString(const std::filesystem::path& file, std::string_view plaintext) {
    auto protectedData = ProtectString(plaintext);
    if (!protectedData) return false;
    std::error_code ec;
    if (file.has_parent_path()) std::filesystem::create_directories(file.parent_path(), ec);
    auto tmp = file;
    tmp += ".tmp";
    std::ofstream out(tmp, std::ios::binary | std::ios::trunc);
    if (!out) return false;
    out.write(reinterpret_cast<const char*>(protectedData->data()), static_cast<std::streamsize>(protectedData->size()));
    out.close();
    if (!out) return false;
    std::filesystem::rename(tmp, file, ec);
    if (ec) {
        std::filesystem::remove(file, ec);
        ec.clear();
        std::filesystem::rename(tmp, file, ec);
    }
#ifdef _WIN32
    if (!ec) SetFileAttributesW(file.wstring().c_str(), FILE_ATTRIBUTE_HIDDEN);
#endif
    return !ec;
}

std::optional<std::string> LoadProtectedString(const std::filesystem::path& file) {
    std::ifstream in(file, std::ios::binary);
    if (!in) return std::nullopt;
    std::vector<uint8_t> data((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
    return UnprotectString(data);
}

} // namespace SffCore::SecretStore
