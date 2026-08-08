#pragma once

#include <cstdint>
#include <filesystem>
#include <optional>
#include <string>

namespace SffCore::Integrity {
struct VerificationResult {
    bool ok = false;
    std::string message;
};

bool VerifyFileSize(const std::filesystem::path& file, std::optional<uint64_t> expectedSize = std::nullopt);
bool VerifyManifestMagic(const std::filesystem::path& file);
bool VerifyManifestParseable(const std::filesystem::path& file);
std::optional<std::string> ComputeSha256(const std::filesystem::path& file);
bool VerifySha256(const std::filesystem::path& file, std::string_view expectedHex);
VerificationResult VerifyManifestFull(const std::filesystem::path& file,
                                      std::optional<uint64_t> expectedSize = std::nullopt,
                                      std::optional<std::string_view> expectedSha256 = std::nullopt);
bool HandleVerificationFailure(const std::filesystem::path& file, bool removeFile = true);
} // namespace SffCore::Integrity
