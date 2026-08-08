#pragma once

#include <map>
#include <optional>
#include <string>
#include <string_view>

namespace SffCore::KeyValues {

class Node {
public:
    using Object = std::map<std::string, Node, std::less<>>;

    Node() = default;
    explicit Node(std::string value) : value_(std::move(value)), object_(false) {}
    explicit Node(Object object) : children_(std::move(object)), object_(true) {}

    bool IsObject() const noexcept { return object_; }
    const std::string& Value() const noexcept { return value_; }
    const Object& Children() const noexcept { return children_; }
    Object& Children() noexcept { return children_; }

    const Node* Find(std::string_view key) const noexcept;
    Node* Find(std::string_view key) noexcept;
    std::optional<std::string> GetString(std::string_view key) const;

private:
    std::string value_;
    Object children_;
    bool object_ = true;
};

std::optional<Node> Parse(std::string_view text);
std::optional<Node> Load(const std::string& path);
std::string Dump(const Node& root);
bool SaveAtomic(const std::string& path, const Node& root);

} // namespace SffCore::KeyValues
