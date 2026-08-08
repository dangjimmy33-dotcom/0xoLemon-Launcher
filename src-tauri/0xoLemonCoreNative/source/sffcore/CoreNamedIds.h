#pragma once

#include <filesystem>
#include <map>
#include <string>

namespace SffCore::NamedIds {
using Registry = std::map<std::string, std::string, std::less<>>;
Registry Load(const std::filesystem::path& folder);
bool Save(const std::filesystem::path& folder, const Registry& registry);
} // namespace SffCore::NamedIds
