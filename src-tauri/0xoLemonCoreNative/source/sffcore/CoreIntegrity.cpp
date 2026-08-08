#include "sffcore/CoreIntegrity.h"

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <bcrypt.h>
#endif

#include <array>
#include <cctype>
#include <fstream>
#include <iomanip>
#include <sstream>
#include <vector>

namespace SffCore::Integrity {
namespace {
constexpr std::array<unsigned char, 4> kManifestMagic{0x27, 0x44, 0x56, 0x01};
std::string Lower(std::string_view s) {
    std::string out(s);
    for (char& c : out) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    return out;
}
} // namespace

bool VerifyFileSize(const std::filesystem::path& file, std::optional<uint64_t> expectedSize) {
    if (!expectedSize) return true;
    std::error_code ec;
    const auto size = std::filesystem::file_size(file, ec);
    return !ec && size == *expectedSize;
}

bool VerifyManifestMagic(const std::filesystem::path& file) {
    std::ifstream in(file, std::ios::binary);
    if (!in) return false;
    std::array<unsigned char, 4> magic{};
    if (!in.read(reinterpret_cast<char*>(magic.data()), static_cast<std::streamsize>(magic.size()))) return false;
    return magic == kManifestMagic;
}

bool VerifyManifestParseable(const std::filesystem::path& file) {
    if (!VerifyManifestMagic(file)) return false;
    std::error_code ec;
    return std::filesystem::file_size(file, ec) >= 24 && !ec;
}

std::optional<std::string> ComputeSha256(const std::filesystem::path& file) {
#ifdef _WIN32
    BCRYPT_ALG_HANDLE alg = nullptr;
    BCRYPT_HASH_HANDLE hash = nullptr;
    DWORD objectSize = 0, cb = 0, hashSize = 0;
    std::vector<unsigned char> object;
    std::vector<unsigned char> digest;
    auto cleanup = [&] {
        if (hash) BCryptDestroyHash(hash);
        if (alg) BCryptCloseAlgorithmProvider(alg, 0);
    };

    if (BCryptOpenAlgorithmProvider(&alg, BCRYPT_SHA256_ALGORITHM, nullptr, 0) < 0) { cleanup(); return std::nullopt; }
    if (BCryptGetProperty(alg, BCRYPT_OBJECT_LENGTH, reinterpret_cast<PUCHAR>(&objectSize), sizeof(objectSize), &cb, 0) < 0 ||
        BCryptGetProperty(alg, BCRYPT_HASH_LENGTH, reinterpret_cast<PUCHAR>(&hashSize), sizeof(hashSize), &cb, 0) < 0) {
        cleanup(); return std::nullopt;
    }
    object.resize(objectSize);
    digest.resize(hashSize);
    if (BCryptCreateHash(alg, &hash, object.data(), objectSize, nullptr, 0, 0) < 0) { cleanup(); return std::nullopt; }

    std::ifstream in(file, std::ios::binary);
    if (!in) { cleanup(); return std::nullopt; }
    std::array<unsigned char, 8192> buffer{};
    while (in) {
        in.read(reinterpret_cast<char*>(buffer.data()), static_cast<std::streamsize>(buffer.size()));
        const auto got = in.gcount();
        if (got > 0 && BCryptHashData(hash, buffer.data(), static_cast<ULONG>(got), 0) < 0) { cleanup(); return std::nullopt; }
    }
    if (!in.eof()) { cleanup(); return std::nullopt; }
    if (BCryptFinishHash(hash, digest.data(), hashSize, 0) < 0) { cleanup(); return std::nullopt; }
    cleanup();

    std::ostringstream out;
    out << std::hex << std::setfill('0');
    for (unsigned char b : digest) out << std::setw(2) << static_cast<unsigned int>(b);
    return out.str();
#else
    (void)file;
    return std::nullopt;
#endif
}

bool VerifySha256(const std::filesystem::path& file, std::string_view expectedHex) {
    auto actual = ComputeSha256(file);
    return actual && Lower(*actual) == Lower(expectedHex);
}

VerificationResult VerifyManifestFull(const std::filesystem::path& file,
                                      std::optional<uint64_t> expectedSize,
                                      std::optional<std::string_view> expectedSha256) {
    std::error_code ec;
    if (!std::filesystem::exists(file, ec)) return {false, "File not found"};
    if (!VerifyFileSize(file, expectedSize)) return {false, "File size mismatch"};
    if (!VerifyManifestMagic(file)) return {false, "Invalid manifest magic bytes"};
    if (!VerifyManifestParseable(file)) return {false, "Manifest file is corrupted or too small"};
    if (expectedSha256 && !VerifySha256(file, *expectedSha256)) return {false, "SHA-256 mismatch"};
    return {true, "Verification successful"};
}

bool HandleVerificationFailure(const std::filesystem::path& file, bool removeFile) {
    if (!removeFile) return true;
    std::error_code ec;
    return std::filesystem::remove(file, ec) || (!ec && !std::filesystem::exists(file));
}

} // namespace SffCore::Integrity
