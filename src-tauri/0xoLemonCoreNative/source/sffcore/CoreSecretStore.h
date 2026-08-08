#pragma once

#include <cstdint>
#include <filesystem>
#include <optional>
#include <span>
#include <string>
#include <vector>

namespace SffCore::SecretStore {
std::optional<std::vector<uint8_t>> ProtectString(std::string_view plaintext);
std::optional<std::string> UnprotectString(std::span<const uint8_t> ciphertext);
bool SaveProtectedString(const std::filesystem::path& file, std::string_view plaintext);
std::optional<std::string> LoadProtectedString(const std::filesystem::path& file);
} // namespace SffCore::SecretStore
