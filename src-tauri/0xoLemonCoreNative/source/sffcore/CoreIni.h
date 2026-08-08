#pragma once

#include <filesystem>
#include <functional>
#include <optional>
#include <string>
#include <string_view>

namespace SffCore::Ini {
using Converter = std::function<std::string(std::string_view)>;
std::optional<std::string> EditOption(const std::filesystem::path& iniFile,
                                      std::string_view section,
                                      std::string_view option,
                                      const Converter& converter);
} // namespace SffCore::Ini
