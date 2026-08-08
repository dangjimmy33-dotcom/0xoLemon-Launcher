#include "sffcore/KeyValues.h"

#include <cctype>
#include <filesystem>
#include <fstream>
#include <sstream>
#include <system_error>

namespace SffCore::KeyValues {
namespace {

class Parser {
public:
    explicit Parser(std::string_view text) : text_(text) {}

    std::optional<Node> Run() {
        Node::Object root;
        if (!ParseObject(root, false)) return std::nullopt;
        SkipSpaceAndComments();
        if (pos_ != text_.size()) return std::nullopt;
        return Node(std::move(root));
    }

private:
    void SkipSpaceAndComments() {
        while (pos_ < text_.size()) {
            if (std::isspace(static_cast<unsigned char>(text_[pos_]))) {
                ++pos_;
                continue;
            }
            if (pos_ + 1 < text_.size() && text_[pos_] == '/' && text_[pos_ + 1] == '/') {
                pos_ += 2;
                while (pos_ < text_.size() && text_[pos_] != '\n') ++pos_;
                continue;
            }
            break;
        }
    }

    std::optional<std::string> Token() {
        SkipSpaceAndComments();
        if (pos_ >= text_.size()) return std::nullopt;
        if (text_[pos_] == '"') {
            ++pos_;
            std::string out;
            while (pos_ < text_.size()) {
                char c = text_[pos_++];
                if (c == '"') return out;
                if (c == '\\' && pos_ < text_.size()) {
                    char e = text_[pos_++];
                    switch (e) {
                    case 'n': out.push_back('\n'); break;
                    case 'r': out.push_back('\r'); break;
                    case 't': out.push_back('\t'); break;
                    case '\\': out.push_back('\\'); break;
                    case '"': out.push_back('"'); break;
                    default: out.push_back('\\'); out.push_back(e); break;
                    }
                } else {
                    out.push_back(c);
                }
            }
            return std::nullopt;
        }

        const size_t begin = pos_;
        while (pos_ < text_.size()) {
            char c = text_[pos_];
            if (std::isspace(static_cast<unsigned char>(c)) || c == '{' || c == '}') break;
            ++pos_;
        }
        if (begin == pos_) return std::nullopt;
        return std::string(text_.substr(begin, pos_ - begin));
    }

    bool ParseObject(Node::Object& out, bool expectClosingBrace) {
        while (true) {
            SkipSpaceAndComments();
            if (pos_ >= text_.size()) return !expectClosingBrace;
            if (text_[pos_] == '}') {
                if (!expectClosingBrace) return false;
                ++pos_;
                return true;
            }

            auto key = Token();
            if (!key) return false;
            SkipSpaceAndComments();
            if (pos_ >= text_.size()) return false;

            if (text_[pos_] == '{') {
                ++pos_;
                Node::Object child;
                if (!ParseObject(child, true)) return false;
                out.insert_or_assign(std::move(*key), Node(std::move(child)));
            } else {
                auto value = Token();
                if (!value) return false;
                out.insert_or_assign(std::move(*key), Node(std::move(*value)));
            }
        }
    }

    std::string_view text_;
    size_t pos_ = 0;
};

std::string Escape(std::string_view text) {
    std::string out;
    out.reserve(text.size() + 8);
    for (char c : text) {
        switch (c) {
        case '\\': out += "\\\\"; break;
        case '"': out += "\\\""; break;
        case '\n': out += "\\n"; break;
        case '\r': out += "\\r"; break;
        case '\t': out += "\\t"; break;
        default: out.push_back(c); break;
        }
    }
    return out;
}

void DumpNode(std::ostringstream& out, const Node& node, int depth) {
    const std::string indent(static_cast<size_t>(depth) * 4, ' ');
    for (const auto& [key, child] : node.Children()) {
        out << indent << '"' << Escape(key) << '"';
        if (child.IsObject()) {
            out << "\n" << indent << "{\n";
            DumpNode(out, child, depth + 1);
            out << indent << "}\n";
        } else {
            out << "\t\t\"" << Escape(child.Value()) << "\"\n";
        }
    }
}

} // namespace

const Node* Node::Find(std::string_view key) const noexcept {
    if (!object_) return nullptr;
    auto it = children_.find(key);
    return it == children_.end() ? nullptr : &it->second;
}

Node* Node::Find(std::string_view key) noexcept {
    if (!object_) return nullptr;
    auto it = children_.find(key);
    return it == children_.end() ? nullptr : &it->second;
}

std::optional<std::string> Node::GetString(std::string_view key) const {
    const Node* node = Find(key);
    if (!node || node->IsObject()) return std::nullopt;
    return node->Value();
}

std::optional<Node> Parse(std::string_view text) {
    return Parser(text).Run();
}

std::optional<Node> Load(const std::string& path) {
    std::ifstream in(path, std::ios::binary);
    if (!in) return std::nullopt;
    std::string text((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
    return Parse(text);
}

std::string Dump(const Node& root) {
    if (!root.IsObject()) return {};
    std::ostringstream out;
    DumpNode(out, root, 0);
    return out.str();
}

bool SaveAtomic(const std::string& path, const Node& root) {
    const std::filesystem::path target(path);
    std::error_code ec;
    if (target.has_parent_path()) std::filesystem::create_directories(target.parent_path(), ec);
    auto tmp = target;
    tmp += ".tmp";
    {
        std::ofstream out(tmp, std::ios::binary | std::ios::trunc);
        if (!out) return false;
        const std::string text = Dump(root);
        out.write(text.data(), static_cast<std::streamsize>(text.size()));
        if (!out) return false;
    }
    std::filesystem::rename(tmp, target, ec);
    if (!ec) return true;
    std::filesystem::remove(target, ec);
    ec.clear();
    std::filesystem::rename(tmp, target, ec);
    return !ec;
}

} // namespace SffCore::KeyValues
