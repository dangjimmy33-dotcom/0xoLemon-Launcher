#include "sffcore/CoreIni.h"

#include <algorithm>
#include <cctype>
#include <fstream>
#include <vector>

namespace SffCore::Ini {
namespace {
std::string_view Trim(std::string_view s) {
    while (!s.empty() && std::isspace(static_cast<unsigned char>(s.front()))) s.remove_prefix(1);
    while (!s.empty() && std::isspace(static_cast<unsigned char>(s.back()))) s.remove_suffix(1);
    return s;
}

bool EqI(std::string_view a, std::string_view b) {
    if (a.size() != b.size()) return false;
    for (size_t i = 0; i < a.size(); ++i)
        if (std::tolower(static_cast<unsigned char>(a[i])) != std::tolower(static_cast<unsigned char>(b[i]))) return false;
    return true;
}
} // namespace

std::optional<std::string> EditOption(const std::filesystem::path& iniFile,
                                      std::string_view section,
                                      std::string_view option,
                                      const Converter& converter) {
    std::ifstream in(iniFile);
    if (!in) return std::nullopt;
    std::vector<std::string> lines;
    std::string line;
    while (std::getline(in, line)) lines.push_back(line);

    bool inSection = false;
    bool changed = false;
    std::string converted;
    for (auto& current : lines) {
        auto trimmed = Trim(current);
        if (trimmed.size() >= 2 && trimmed.front() == '[' && trimmed.back() == ']') {
            inSection = EqI(Trim(trimmed.substr(1, trimmed.size() - 2)), section);
            continue;
        }
        if (!inSection || trimmed.empty() || trimmed.front() == ';' || trimmed.front() == '#') continue;
        const auto eq = current.find('=');
        if (eq == std::string::npos) continue;
        const auto key = Trim(std::string_view(current).substr(0, eq));
        if (!EqI(key, option)) continue;

        const auto rhsView = std::string_view(current).substr(eq + 1);
        const auto value = Trim(rhsView);
        converted = converter(value);

        const size_t valueStart = static_cast<size_t>(value.data() - current.data());
        const size_t valueEnd = valueStart + value.size();
        current.replace(valueStart, valueEnd - valueStart, converted);
        changed = true;
        break;
    }
    if (!changed) return std::nullopt;

    auto tmp = iniFile;
    tmp += ".tmp";
    std::ofstream out(tmp, std::ios::trunc);
    if (!out) return std::nullopt;
    for (size_t i = 0; i < lines.size(); ++i) {
        out << lines[i];
        if (i + 1 < lines.size()) out << '\n';
    }
    out.close();
    if (!out) return std::nullopt;

    std::error_code ec;
    std::filesystem::rename(tmp, iniFile, ec);
    if (ec) {
        std::filesystem::remove(iniFile, ec);
        ec.clear();
        std::filesystem::rename(tmp, iniFile, ec);
    }
    if (ec) return std::nullopt;
    return converted;
}

} // namespace SffCore::Ini
