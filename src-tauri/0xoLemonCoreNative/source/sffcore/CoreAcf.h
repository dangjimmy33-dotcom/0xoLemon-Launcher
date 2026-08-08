#pragma once

#include "sffcore/CoreTypes.h"
#include <filesystem>
#include <optional>
#include <vector>

namespace SffCore::Acf {

struct ParsedAcf {
    AcfInfo info;
    std::filesystem::path path;
    std::filesystem::path libraryRoot;
};

std::vector<std::filesystem::path> GetSteamLibraries(const std::filesystem::path& steamPath);
std::optional<ParsedAcf> ParseFile(const std::filesystem::path& acfPath,
                                   const std::filesystem::path& libraryRoot = {});
std::optional<ParsedAcf> FindAndParse(const std::filesystem::path& steamPath, uint32_t appId);
std::string GetAppName(const std::filesystem::path& steamPath, uint32_t appId);
bool EnsureLibraryHasApp(const std::filesystem::path& steamPath,
                         const std::filesystem::path& libraryPath,
                         uint32_t appId);

} // namespace SffCore::Acf
