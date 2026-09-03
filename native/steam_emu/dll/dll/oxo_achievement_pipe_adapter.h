/*
 * 0xoLemon achievement transport adapter.
 *
 * This file is an LGPL-compatible addition to the bundled GSE source. It is
 * deliberately isolated from Steam API behavior: Steam_User_Stats supplies a
 * state snapshot and a command callback, while this class owns only the
 * authenticated, bounded, reconnecting named-pipe protocol.
 */
#ifndef __INCLUDED_OXO_ACHIEVEMENT_PIPE_ADAPTER_H__
#define __INCLUDED_OXO_ACHIEVEMENT_PIPE_ADAPTER_H__

#include <atomic>
#include <condition_variable>
#include <cstdint>
#include <deque>
#include <functional>
#include <memory>
#include <mutex>
#include <thread>
#include <unordered_map>
#include <unordered_set>

class OxoAchievementPipeAdapter final {
public:
    using CommandHandler = std::function<nlohmann::json(const nlohmann::json &)>;

    static std::unique_ptr<OxoAchievementPipeAdapter> create(
        uint32_t app_id,
        nlohmann::json initial_state,
        CommandHandler command_handler)
    {
#if defined(__WINDOWS__)
        auto adapter = std::unique_ptr<OxoAchievementPipeAdapter>(
            new OxoAchievementPipeAdapter(app_id, std::move(command_handler)));
        if (!adapter->configure_from_environment()) return nullptr;

        initial_state["type"] = "schemaReady";
        adapter->emit_event(std::move(initial_state));
        adapter->worker_ = std::thread([instance = adapter.get()]() { instance->run(); });
        return adapter;
#else
        (void)app_id;
        (void)initial_state;
        (void)command_handler;
        return nullptr;
#endif
    }

    ~OxoAchievementPipeAdapter()
    {
        stop();
    }

    OxoAchievementPipeAdapter(const OxoAchievementPipeAdapter &) = delete;
    OxoAchievementPipeAdapter &operator=(const OxoAchievementPipeAdapter &) = delete;

    void emit_event(const std::string &type, nlohmann::json payload = nlohmann::json::object())
    {
        payload["type"] = type;
        emit_event(std::move(payload));
    }

    void emit_event(nlohmann::json payload)
    {
#if defined(__WINDOWS__)
        if (!enabled_.load() || stop_requested_.load()) return;

        OutboundMessage outbound;
        outbound.message_id = next_client_message_id_.fetch_add(1);
        outbound.envelope = identity_envelope("event", outbound.message_id);
        outbound.envelope["event"] = std::move(payload);

        {
            std::lock_guard<std::mutex> lock(queue_mutex_);
            if (outbound_.size() >= kQueueCapacity) {
                // Keep an already-sent front item for replay. The polling layer
                // is the recovery source for any unsent event evicted here.
                auto victim = outbound_.begin();
                if (victim != outbound_.end() && victim->sent && outbound_.size() > 1) ++victim;
                if (victim != outbound_.end()) outbound_.erase(victim);
                ++dropped_events_;
            }
            if (dropped_events_) {
                outbound.envelope["event"]["droppedEvents"] = dropped_events_;
                dropped_events_ = 0;
            }
            outbound_.push_back(std::move(outbound));
        }
        queue_cv_.notify_one();
#else
        (void)payload;
#endif
    }

    void stop()
    {
#if defined(__WINDOWS__)
        if (!enabled_.load()) return;
        bool expected = false;
        if (stop_requested_.load()) return;
        emit_event(std::string("runtimeStopped"));
        if (!stop_requested_.compare_exchange_strong(expected, true)) return;
        queue_cv_.notify_all();
        if (worker_.joinable()) worker_.join();
        enabled_.store(false);
#endif
    }

private:
    explicit OxoAchievementPipeAdapter(uint32_t app_id, CommandHandler command_handler)
        : app_id_(app_id), command_handler_(std::move(command_handler))
    {
    }

#if defined(__WINDOWS__)
    static constexpr uint32_t kProtocolVersion = 1;
    static constexpr size_t kMaxMessageBytes = 64 * 1024;
    static constexpr size_t kQueueCapacity = 256;
    static constexpr size_t kReplayCapacity = 512;

    struct OutboundMessage {
        uint64_t message_id{};
        nlohmann::json envelope{};
        bool sent{};
    };

    bool configure_from_environment()
    {
        pipe_name_ = get_env_variable("OXO_ACHIEVEMENT_PIPE");
        secret_ = get_env_variable("OXO_ACHIEVEMENT_SECRET");
        session_id_ = get_env_variable("OXO_ACHIEVEMENT_SESSION_ID");
        game_id_ = get_env_variable("OXO_GAME_ID");
        const auto protocol = get_env_variable("OXO_ACHIEVEMENT_PROTOCOL");
        const auto expected_app_id = get_env_variable("OXO_STEAM_APP_ID");

        if (protocol != std::to_string(kProtocolVersion)) return false;
        if (pipe_name_.rfind(R"(\\.\pipe\0xo-ach-)", 0) != 0 || pipe_name_.size() > 240) return false;
        if (secret_.size() != 64 || !std::all_of(secret_.begin(), secret_.end(), [](unsigned char ch) {
                return std::isxdigit(ch) != 0;
            })) return false;
        if (session_id_.empty() || session_id_.size() > 64) return false;
        if (game_id_.empty() || game_id_.size() > 128 ||
            !std::all_of(game_id_.begin(), game_id_.end(), [](unsigned char ch) {
                return std::isalnum(ch) != 0 || ch == '-' || ch == '_';
            })) return false;
        try {
            if (std::stoull(expected_app_id) != app_id_) return false;
        } catch (...) {
            return false;
        }

        process_id_ = GetCurrentProcessId();
        pipe_name_wide_ = utf8_decode(pipe_name_);
        enabled_.store(process_id_ != 0 && !pipe_name_wide_.empty() && static_cast<bool>(command_handler_));
        return enabled_.load();
    }

    nlohmann::json identity_envelope(const char *kind, uint64_t message_id) const
    {
        return {
            {"protocolVersion", kProtocolVersion},
            {"kind", kind},
            {"secret", secret_},
            {"sessionId", session_id_},
            {"gameId", game_id_},
            {"appId", app_id_},
            {"pid", process_id_},
            {"messageId", message_id},
        };
    }

    void run()
    {
        const auto stop_deadline_slack = std::chrono::milliseconds(250);
        std::optional<std::chrono::steady_clock::time_point> stop_deadline;
        HANDLE pipe = INVALID_HANDLE_VALUE;

        while (true) {
            if (stop_requested_.load() && !stop_deadline) {
                stop_deadline = std::chrono::steady_clock::now() + stop_deadline_slack;
            }
            if (stop_deadline && std::chrono::steady_clock::now() >= *stop_deadline) break;

            if (pipe == INVALID_HANDLE_VALUE) {
                pipe = connect_pipe();
                if (pipe == INVALID_HANDLE_VALUE) {
                    std::unique_lock<std::mutex> lock(queue_mutex_);
                    queue_cv_.wait_for(lock, std::chrono::milliseconds(100));
                    continue;
                }
                reset_outbound_for_replay();
                if (!write_json(pipe, hello_envelope())) {
                    CloseHandle(pipe);
                    pipe = INVALID_HANDLE_VALUE;
                    continue;
                }
            }

            nlohmann::json incoming;
            const auto read_result = try_read_json(pipe, incoming);
            if (read_result == ReadResult::Disconnected ||
                (read_result == ReadResult::Message && !handle_incoming(pipe, incoming))) {
                CloseHandle(pipe);
                pipe = INVALID_HANDLE_VALUE;
                continue;
            }

            if (!send_next_event(pipe)) {
                CloseHandle(pipe);
                pipe = INVALID_HANDLE_VALUE;
                continue;
            }

            std::unique_lock<std::mutex> lock(queue_mutex_);
            queue_cv_.wait_for(lock, std::chrono::milliseconds(10));
        }

        if (pipe != INVALID_HANDLE_VALUE) CloseHandle(pipe);
    }

    nlohmann::json hello_envelope() const
    {
        auto hello = identity_envelope("hello", 1);
        hello["capabilities"] = {
            "schemaReady", "unlocked", "cleared", "progress", "statChanged", "flushed",
            "runtimeStopped", "queryState", "unlock", "clear", "setProgress", "setStat", "flush"
        };
        return hello;
    }

    HANDLE connect_pipe() const
    {
        HANDLE pipe = CreateFileW(
            pipe_name_wide_.c_str(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            nullptr,
            OPEN_EXISTING,
            0,
            nullptr);
        if (pipe == INVALID_HANDLE_VALUE && GetLastError() == ERROR_PIPE_BUSY) {
            if (WaitNamedPipeW(pipe_name_wide_.c_str(), 250)) {
                pipe = CreateFileW(
                    pipe_name_wide_.c_str(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    nullptr,
                    OPEN_EXISTING,
                    0,
                    nullptr);
            }
        }
        if (pipe == INVALID_HANDLE_VALUE) return INVALID_HANDLE_VALUE;

        DWORD mode = PIPE_READMODE_MESSAGE;
        if (!SetNamedPipeHandleState(pipe, &mode, nullptr, nullptr)) {
            CloseHandle(pipe);
            return INVALID_HANDLE_VALUE;
        }
        return pipe;
    }

    enum class ReadResult { None, Message, Disconnected };

    ReadResult try_read_json(HANDLE pipe, nlohmann::json &message) const
    {
        DWORD available = 0;
        DWORD message_bytes = 0;
        if (!PeekNamedPipe(pipe, nullptr, 0, nullptr, &available, &message_bytes)) {
            return ReadResult::Disconnected;
        }
        if (!available) return ReadResult::None;
        if (!message_bytes) message_bytes = available;
        if (message_bytes > kMaxMessageBytes) return ReadResult::Disconnected;

        std::string bytes(message_bytes, '\0');
        DWORD read = 0;
        if (!ReadFile(pipe, bytes.data(), message_bytes, &read, nullptr) || !read) {
            return ReadResult::Disconnected;
        }
        bytes.resize(read);
        try {
            message = nlohmann::json::parse(bytes);
        } catch (...) {
            return ReadResult::Disconnected;
        }
        return message.is_object() ? ReadResult::Message : ReadResult::Disconnected;
    }

    bool write_json(HANDLE pipe, const nlohmann::json &message) const
    {
        std::string bytes;
        try {
            bytes = message.dump();
        } catch (...) {
            return false;
        }
        if (bytes.empty() || bytes.size() > kMaxMessageBytes) return false;
        DWORD written = 0;
        return WriteFile(
                   pipe,
                   bytes.data(),
                   static_cast<DWORD>(bytes.size()),
                   &written,
                   nullptr) != FALSE && written == static_cast<DWORD>(bytes.size());
    }

    bool validate_identity(const nlohmann::json &message) const
    {
        try {
            return message.value("protocolVersion", 0u) == kProtocolVersion &&
                message.value("secret", std::string{}) == secret_ &&
                message.value("sessionId", std::string{}) == session_id_ &&
                message.value("gameId", std::string{}) == game_id_ &&
                message.value("appId", 0u) == app_id_ &&
                message.value("pid", 0u) == process_id_ &&
                message.value("messageId", uint64_t{}) > 0;
        } catch (...) {
            return false;
        }
    }

    bool handle_incoming(HANDLE pipe, const nlohmann::json &message)
    {
        if (!validate_identity(message)) return false;
        const auto kind = message.value("kind", std::string{});
        if (kind == "ack") {
            acknowledge_event(message.value("ackId", uint64_t{}));
            return true;
        }
        if (kind != "command" || !message.contains("command") || !message["command"].is_object()) {
            return false;
        }

        const auto command_id = message.value("messageId", uint64_t{});
        {
            std::lock_guard<std::mutex> lock(command_mutex_);
            auto cached = command_ack_cache_.find(command_id);
            if (cached != command_ack_cache_.end()) return write_json(pipe, cached->second);
            if (command_id <= highest_command_id_) return false;
        }

        nlohmann::json callback_result;
        try {
            callback_result = command_handler_(message["command"]);
        } catch (const std::exception &error) {
            callback_result = {{"ok", false}, {"error", error.what()}};
        } catch (...) {
            callback_result = {{"ok", false}, {"error", "Unhandled GSE command error"}};
        }

        auto ack = identity_envelope("ack", next_client_message_id_.fetch_add(1));
        ack["ackId"] = command_id;
        ack["ok"] = callback_result.value("ok", false);
        ack["result"] = callback_result.value("result", nlohmann::json{});
        ack["error"] = callback_result.value("error", std::string{});

        {
            std::lock_guard<std::mutex> lock(command_mutex_);
            highest_command_id_ = command_id;
            command_ack_cache_[command_id] = ack;
            command_ack_order_.push_back(command_id);
            while (command_ack_order_.size() > kReplayCapacity) {
                const auto expired = command_ack_order_.front();
                command_ack_order_.pop_front();
                command_ack_cache_.erase(expired);
            }
        }
        return write_json(pipe, ack);
    }

    bool send_next_event(HANDLE pipe)
    {
        nlohmann::json message;
        {
            std::lock_guard<std::mutex> lock(queue_mutex_);
            if (outbound_.empty() || outbound_.front().sent) return true;
            outbound_.front().sent = true;
            message = outbound_.front().envelope;
        }
        if (write_json(pipe, message)) return true;
        std::lock_guard<std::mutex> lock(queue_mutex_);
        if (!outbound_.empty()) outbound_.front().sent = false;
        return false;
    }

    void acknowledge_event(uint64_t message_id)
    {
        if (!message_id) return;
        std::lock_guard<std::mutex> lock(queue_mutex_);
        if (!outbound_.empty() && outbound_.front().message_id == message_id) {
            outbound_.pop_front();
            queue_cv_.notify_one();
        }
    }

    void reset_outbound_for_replay()
    {
        std::lock_guard<std::mutex> lock(queue_mutex_);
        if (!outbound_.empty()) outbound_.front().sent = false;
    }

    uint32_t app_id_{};
    uint32_t process_id_{};
    std::string pipe_name_{};
    std::wstring pipe_name_wide_{};
    std::string secret_{};
    std::string session_id_{};
    std::string game_id_{};
    CommandHandler command_handler_{};
    std::atomic<bool> enabled_{false};
    std::atomic<bool> stop_requested_{false};
    std::atomic<uint64_t> next_client_message_id_{2};
    std::thread worker_{};

    std::mutex queue_mutex_{};
    std::condition_variable queue_cv_{};
    std::deque<OutboundMessage> outbound_{};
    uint64_t dropped_events_{};

    std::mutex command_mutex_{};
    uint64_t highest_command_id_{};
    std::deque<uint64_t> command_ack_order_{};
    std::unordered_map<uint64_t, nlohmann::json> command_ack_cache_{};
#else
    uint32_t app_id_{};
    CommandHandler command_handler_{};
#endif
};

#endif // __INCLUDED_OXO_ACHIEVEMENT_PIPE_ADAPTER_H__
