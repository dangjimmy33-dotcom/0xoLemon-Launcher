#include "overlay/steam_overlay_stats.h"
// translation
#include "overlay/steam_overlay_translations.h"
#include <utility>
#include <algorithm>
#include <cmath>

#if defined(_WIN32)
#include <windows.h>
#endif

// Returns true if the game window is the currently focused foreground window.
// On non-Windows platforms, always returns true (no focus-based pausing).
static bool is_game_focused()
{
#if defined(_WIN32)
    HWND fg = GetForegroundWindow();
    if (!fg) return false;
    DWORD pid = 0;
    GetWindowThreadProcessId(fg, &pid);
    return pid == GetCurrentProcessId();
#else
    return true;
#endif
}

// EMA smoothing factor (0..1). Lower = smoother but slower to react.
static constexpr float EMA_ALPHA = 0.1f;

Steam_Overlay_Stats::Steam_Overlay_Stats(class Settings* settings):
    settings(settings)
{
    show_fps = settings->overlay_always_show_fps;
    show_frametime = settings->overlay_always_show_frametime;
    show_playtime = settings->overlay_always_show_playtime;
    show_fps_graph = settings->overlay_show_fps_graph;
    show_frametime_graph = settings->overlay_show_frametime_graph;
    show_min_max_avg = settings->overlay_show_min_max_avg;
    show_percentile_1 = settings->overlay_show_percentile_1;
    show_percentile_5 = settings->overlay_show_percentile_5;
    show_percentile_01 = settings->overlay_show_percentile_01;
    graph_timeframe_sec = settings->overlay_graph_timeframe_sec;
}

bool Steam_Overlay_Stats::show_any_stats() const
{
    return show_fps || show_frametime || show_playtime;
}

void Steam_Overlay_Stats::update_frametime(const std::chrono::high_resolution_clock::time_point &now)
{
    float dt_ms = std::chrono::duration<float, std::milli>(now - last_frame_timepoint).count();
    last_frame_timepoint = now;

    // Clamp absurd values (e.g. first frame, or debugger pause)
    if (dt_ms < 0.0f) dt_ms = 0.0f;
    if (dt_ms > 1000.0f) dt_ms = 1000.0f;

    // Store in ring buffer
    frametime_history[frametime_history_idx] = dt_ms;
    frametime_history_idx = (frametime_history_idx + 1) % FRAMETIME_HISTORY_SIZE;
    if (frametime_history_count < FRAMETIME_HISTORY_SIZE) frametime_history_count++;

    // EMA smoothing
    if (smoothed_frametime_ms <= 0.0f) {
        smoothed_frametime_ms = dt_ms; // seed on first frame
    } else {
        smoothed_frametime_ms = EMA_ALPHA * dt_ms + (1.0f - EMA_ALPHA) * smoothed_frametime_ms;
    }
    smoothed_fps = (smoothed_frametime_ms > 0.0f) ? (1000.0f / smoothed_frametime_ms) : 0.0f;

    // Min/max/avg over visible window
    int vis_count = get_visible_frame_count();
    float sum = 0.0f;
    float mn = 1e9f;
    float mx = 0.0f;
    int ring_start = (frametime_history_idx - vis_count + FRAMETIME_HISTORY_SIZE) % FRAMETIME_HISTORY_SIZE;
    for (int i = 0; i < vis_count; i++) {
        float v = frametime_history[(ring_start + i) % FRAMETIME_HISTORY_SIZE];
        sum += v;
        if (v < mn) mn = v;
        if (v > mx) mx = v;
    }
    if (vis_count > 0) {
        avg_frametime_ms = sum / vis_count;
        min_frametime_ms = mn;
        max_frametime_ms = mx;
    }

    // Snapshot display values every 500ms
    auto display_elapsed = std::chrono::duration<float, std::milli>(now - last_display_update).count();
    if (display_elapsed >= 500.0f || display_fps <= 0.0f) {
        last_display_update = now;
        display_fps = smoothed_fps;
        display_frametime_ms = smoothed_frametime_ms;
        display_min_ft = min_frametime_ms;
        display_max_ft = max_frametime_ms;
        display_avg_ft = avg_frametime_ms;
    }
}

int Steam_Overlay_Stats::get_visible_frame_count() const
{
    if (frametime_history_count <= 0) return 0;
    float budget_ms = graph_timeframe_sec * 1000.0f;
    float accum = 0.0f;
    int n = 0;
    for (int i = 0; i < frametime_history_count; i++) {
        int idx = (frametime_history_idx - 1 - i + FRAMETIME_HISTORY_SIZE) % FRAMETIME_HISTORY_SIZE;
        accum += frametime_history[idx];
        n++;
        if (accum >= budget_ms) break;
    }
    return n;
}

void Steam_Overlay_Stats::update_playtime(const std::chrono::high_resolution_clock::time_point &now)
{
    const auto update_duration_sec = std::chrono::duration_cast<std::chrono::seconds>(
        now - last_playtime
    ).count();
    if (update_duration_sec < 1) return;

    // Pause accumulation when the game window loses focus (ALT+TAB)
    if (!is_game_focused() && settings->pause_session_when_unfocused) {
        if (!playtime_pause_start.has_value()) {
            playtime_pause_start = now;
        }
        return; // freeze the display values
    }

    // Accumulate paused duration when focus returns
    if (playtime_pause_start.has_value()) {
        total_playtime_paused += now - *playtime_pause_start;
        playtime_pause_start.reset();
    }

    last_playtime = now;

    const auto effective_elapsed = (now - initial_time) - total_playtime_paused;
    const auto time_duration_sec = (unsigned long long)std::chrono::duration_cast<std::chrono::seconds>(
        effective_elapsed
    ).count();
    active_playtime_sec = static_cast<unsigned>(time_duration_sec % 60);

    const auto time_duration_min = time_duration_sec / 60;
    active_playtime_min = static_cast<unsigned>(time_duration_min % 60);

    const auto time_duration_hr = time_duration_min / 60;
    active_playtime_hr = static_cast<unsigned>(time_duration_hr % 24);
}

void Steam_Overlay_Stats::render_stats(int current_language)
{
    auto now = std::chrono::high_resolution_clock::now();
    if (show_fps || show_frametime) {
        update_frametime(now);
    }
    if (show_playtime) {
        update_playtime(now);
    }

    ImGui::PushFont(font, 0.0f);

    ImGui::PushStyleVar(ImGuiStyleVar_WindowRounding, settings->overlay_appearance.notification_rounding);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowBorderSize, 0);

    // Local shorthand: apply swapchain colour transform if one is set.
    auto tc = [this](const ImVec4 &c) -> ImVec4 { return color_transform ? color_transform(c) : c; };

    ImGui::PushStyleColor(ImGuiCol_WindowBg, tc(ImVec4(
        settings->overlay_appearance.stats_background_r,
        settings->overlay_appearance.stats_background_g,
        settings->overlay_appearance.stats_background_b,
        settings->overlay_appearance.stats_background_a
    )));
    ImGui::PushStyleColor(ImGuiCol_Text, tc(ImVec4(
        settings->overlay_appearance.stats_text_r,
        settings->overlay_appearance.stats_text_g,
        settings->overlay_appearance.stats_text_b,
        settings->overlay_appearance.stats_text_a
    )));
    
    // Build the main stats text line
    std::stringstream stats_txt_buff{};
    if (show_fps) {
        stats_txt_buff << translationFpsDisplay[current_language]
                       << std::left << std::setw(4) << std::fixed << std::setprecision(1)
                       << display_fps
                       << std::defaultfloat << std::right << std::setw(0);
    }
    if (show_frametime) {
        if (stats_txt_buff.tellp() > 0) {
            stats_txt_buff << " | ";
        }
        stats_txt_buff << translationFrametimeDisplay[current_language]
                       << std::left << std::setw(5) << std::fixed << std::setprecision(1)
                       << display_frametime_ms
                       << std::defaultfloat << std::right << std::setw(0)
                       << translationFrametimeUnitDisplay[current_language];
    }
    if (show_playtime) {
        if (stats_txt_buff.tellp() > 0) {
            stats_txt_buff << " | ";
        }
        const auto org_fill = stats_txt_buff.fill();
        stats_txt_buff << translationPlaytimeDisplay[current_language]
                       << std::setw(2) << std::setfill('0')
                       << active_playtime_hr << ':'
                       << std::setw(2) << std::setfill('0')
                       << active_playtime_min << ':'
                       << std::setw(2) << std::setfill('0')
                       << active_playtime_sec
                       << std::setw(0) << std::setfill(org_fill);
    }
    const auto stats_txt = stats_txt_buff.str();

    // Determine if we need graphs
    bool want_ft_graph = show_frametime && show_frametime_graph;
    bool want_fps_graph = show_fps && show_fps_graph;
    int vis_count = get_visible_frame_count();
    bool show_graph = (want_ft_graph || want_fps_graph) && vis_count > 1;
    float graph_height = 40.0f;

    // Build data arrays from visible window of ring buffer
    std::vector<float> ft_data, fps_data, sorted_ft;
    if (show_graph || show_min_max_avg || show_percentile_1 || show_percentile_5 || show_percentile_01) {
        ft_data.resize(vis_count);
        fps_data.resize(vis_count);
        sorted_ft.resize(vis_count);
        int ring_start = (frametime_history_idx - vis_count + FRAMETIME_HISTORY_SIZE) % FRAMETIME_HISTORY_SIZE;
        for (int i = 0; i < vis_count; i++) {
            float ft = frametime_history[(ring_start + i) % FRAMETIME_HISTORY_SIZE];
            ft_data[i] = ft;
            fps_data[i] = (ft > 0.0f) ? (1000.0f / ft) : 0.0f;
            sorted_ft[i] = ft;
        }
        std::sort(sorted_ft.begin(), sorted_ft.end());
    }

    // Percentile helpers
    auto ft_percentile = [&](float pct) -> float {
        if (vis_count <= 0) return 0.0f;
        int idx = (int)(pct * vis_count) - 1;
        if (idx < 0) idx = 0;
        if (idx >= vis_count) idx = vis_count - 1;
        return sorted_ft[idx];
    };
    auto fps_low = [&](float pct) -> float {
        if (vis_count <= 0) return 0.0f;
        int n = (int)(pct * vis_count);
        if (n < 1) n = 1;
        float sum = 0.0f;
        for (int i = vis_count - n; i < vis_count; i++) sum += sorted_ft[i];
        float avg_ft = sum / n;
        return (avg_ft > 0.0f) ? (1000.0f / avg_ft) : 0.0f;
    };

    // Determine minimum content width for graphs
    float min_content_width = show_graph ? 260.0f : 0.0f;

    auto &io = ImGui::GetIO();
    const float pos_x_norm = settings->overlay_stats_pos_x;
    const float pos_y_norm = settings->overlay_stats_pos_y;

    if (ImGui::Begin("wnd_fps_frametime", nullptr,
            ImGuiWindowFlags_NoMove | ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_AlwaysAutoResize |
            ImGuiWindowFlags_NoFocusOnAppearing | ImGuiWindowFlags_NoBringToFrontOnFocus | ImGuiWindowFlags_NoInputs |
            ImGuiWindowFlags_NoCollapse | ImGuiWindowFlags_NoSavedSettings | ImGuiWindowFlags_NoScrollWithMouse)) {

        // Anchor the window position based on its actual size
        ImVec2 win_sz = ImGui::GetWindowSize();
        float anchor_x = win_sz.x * pos_x_norm;
        float anchor_y = win_sz.y * pos_y_norm;
        ImGui::SetWindowPos(ImVec2(
            io.DisplaySize.x * pos_x_norm - anchor_x,
            io.DisplaySize.y * pos_y_norm - anchor_y
        ));

        float content_width = ImGui::GetContentRegionAvail().x;
        if (content_width < min_content_width) content_width = min_content_width;

        ImGui::TextWrapped("%s", stats_txt.c_str());

        // ---- Frametime graph + stats ----
        if (show_frametime && vis_count > 1) {
            if (show_frametime_graph) {
                ImGui::Spacing();
                ImGui::TextColored(tc(ImVec4(0.8f, 0.8f, 0.4f, 1.0f)), "Frametime");

                float ft_scale_max = max_frametime_ms * 1.2f;
                if (ft_scale_max < 1.0f) ft_scale_max = 1.0f;

                ImGui::PushStyleColor(ImGuiCol_PlotLines, tc(ImVec4(0.4f, 0.8f, 0.4f, 1.0f)));
                ImGui::PushStyleColor(ImGuiCol_FrameBg, tc(ImVec4(0.0f, 0.0f, 0.0f, 0.3f)));
                ImGui::PlotLines("##ft_graph", ft_data.data(), vis_count, 0, nullptr,
                    0.0f, ft_scale_max, ImVec2(content_width, graph_height));
                ImGui::PopStyleColor(2);
            }

            if (show_min_max_avg) {
                ImGui::TextColored(tc(ImVec4(0.7f, 0.7f, 0.7f, 1.0f)),
                    "Min: %.1fms  Avg: %.1fms  Max: %.1fms",
                    display_min_ft, display_avg_ft, display_max_ft);
            }

            bool any_pct = show_percentile_01 || show_percentile_1 || show_percentile_5;
            if (any_pct) {
                std::string pct_line;
                if (show_percentile_01) {
                    char buf[32]; snprintf(buf, sizeof(buf), "0.1%% high: %.1fms", ft_percentile(0.999f));
                    pct_line += buf;
                }
                if (show_percentile_1) {
                    if (!pct_line.empty()) pct_line += "  ";
                    char buf[32]; snprintf(buf, sizeof(buf), "1%% high: %.1fms", ft_percentile(0.99f));
                    pct_line += buf;
                }
                if (show_percentile_5) {
                    if (!pct_line.empty()) pct_line += "  ";
                    char buf[32]; snprintf(buf, sizeof(buf), "5%% high: %.1fms", ft_percentile(0.95f));
                    pct_line += buf;
                }
                ImGui::TextColored(tc(ImVec4(0.7f, 0.7f, 0.7f, 1.0f)), "%s", pct_line.c_str());
            }
        }

        // ---- FPS graph + stats ----
        if (show_fps && vis_count > 1) {
            float fps_min = (display_max_ft > 0.0f) ? (1000.0f / display_max_ft) : 0.0f;
            float fps_max = (display_min_ft > 0.0f) ? (1000.0f / display_min_ft) : 0.0f;
            float fps_avg = (display_avg_ft > 0.0f) ? (1000.0f / display_avg_ft) : 0.0f;

            if (show_fps_graph) {
                ImGui::Spacing();
                ImGui::TextColored(tc(ImVec4(0.8f, 0.8f, 0.4f, 1.0f)), "FPS");

                float fps_scale_max = fps_max * 1.2f;
                if (fps_scale_max < 1.0f) fps_scale_max = 1.0f;

                ImGui::PushStyleColor(ImGuiCol_PlotLines, tc(ImVec4(0.4f, 0.6f, 1.0f, 1.0f)));
                ImGui::PushStyleColor(ImGuiCol_FrameBg, tc(ImVec4(0.0f, 0.0f, 0.0f, 0.3f)));
                ImGui::PlotLines("##fps_graph", fps_data.data(), vis_count, 0, nullptr,
                    0.0f, fps_scale_max, ImVec2(content_width, graph_height));
                ImGui::PopStyleColor(2);
            }

            if (show_min_max_avg) {
                ImGui::TextColored(tc(ImVec4(0.7f, 0.7f, 0.7f, 1.0f)),
                    "Min: %.0f  Avg: %.0f  Max: %.0f",
                    fps_min, fps_avg, fps_max);
            }

            bool any_pct = show_percentile_01 || show_percentile_1 || show_percentile_5;
            if (any_pct) {
                std::string pct_line;
                if (show_percentile_01) {
                    char buf[32]; snprintf(buf, sizeof(buf), "0.1%% Low: %.0f", fps_low(0.001f));
                    pct_line += buf;
                }
                if (show_percentile_1) {
                    if (!pct_line.empty()) pct_line += "  ";
                    char buf[32]; snprintf(buf, sizeof(buf), "1%% Low: %.0f", fps_low(0.01f));
                    pct_line += buf;
                }
                if (show_percentile_5) {
                    if (!pct_line.empty()) pct_line += "  ";
                    char buf[32]; snprintf(buf, sizeof(buf), "5%% Low: %.0f", fps_low(0.05f));
                    pct_line += buf;
                }
                ImGui::TextColored(tc(ImVec4(0.7f, 0.7f, 0.7f, 1.0f)), "%s", pct_line.c_str());
            }
        }
    }
    ImGui::End();

    ImGui::PopStyleColor(2);
    ImGui::PopStyleVar(2);
    ImGui::PopFont();
}

void Steam_Overlay_Stats::render_stats_settings(int current_language)
{
    if (!show_stats_settings) return;

    ImGui::SetNextWindowSize(ImVec2(310.0f, 0.0f), ImGuiCond_FirstUseEver);
    ImGui::SetNextWindowBgAlpha(0.95f);
    if (ImGui::Begin("Performance Stats Settings", &show_stats_settings,
            ImGuiWindowFlags_NoCollapse | ImGuiWindowFlags_NoSavedSettings |
            ImGuiWindowFlags_AlwaysAutoResize)) {

        // --- Master toggles ---
        ImGui::TextColored(ImVec4(0.8f, 0.8f, 0.4f, 1.0f), "Display");
        ImGui::Separator();
        ImGui::Checkbox("FPS", &show_fps);
        ImGui::SameLine();
        ImGui::Checkbox("Frametime", &show_frametime);
        ImGui::SameLine();
        ImGui::Checkbox("Playtime", &show_playtime);

        ImGui::Spacing();

        // --- Graph toggles ---
        ImGui::TextColored(ImVec4(0.8f, 0.8f, 0.4f, 1.0f), "Graphs");
        ImGui::Separator();
        ImGui::Checkbox("FPS Graph", &show_fps_graph);
        ImGui::SameLine();
        ImGui::Checkbox("Frametime Graph", &show_frametime_graph);

        ImGui::Spacing();

        // --- Graph timeframe ---
        ImGui::TextColored(ImVec4(0.8f, 0.8f, 0.4f, 1.0f), "Graph Timeframe");
        ImGui::Separator();
        ImGui::SliderInt("##timeframe", &graph_timeframe_sec, 1, 30, "%d sec");

        ImGui::Spacing();

        // --- Statistics toggles ---
        ImGui::TextColored(ImVec4(0.8f, 0.8f, 0.4f, 1.0f), "Statistics");
        ImGui::Separator();
        ImGui::Checkbox("Min / Max / Avg", &show_min_max_avg);
        ImGui::Checkbox("0.1% Low", &show_percentile_01);
        ImGui::SameLine();
        ImGui::Checkbox("1% Low", &show_percentile_1);
        ImGui::SameLine();
        ImGui::Checkbox("5% Low", &show_percentile_5);
    }
    ImGui::End();
}
