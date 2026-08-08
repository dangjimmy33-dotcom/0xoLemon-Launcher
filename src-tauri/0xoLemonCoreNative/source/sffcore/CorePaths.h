#pragma once

#include <filesystem>

namespace SffCore::Paths {
const std::filesystem::path& ModuleDirectory();
const std::filesystem::path& DataDirectory();
const std::filesystem::path& ManifestStagingDirectory();
} // namespace SffCore::Paths
