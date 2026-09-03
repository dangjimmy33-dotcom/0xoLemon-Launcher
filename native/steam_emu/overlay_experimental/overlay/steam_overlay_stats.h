#ifndef _STEAM_OVERLAY_STATS_H_
#define _STEAM_OVERLAY_STATS_H_

#include <chrono>
#include <optional>
#include <string_view>
#include <unordered_map>
#include <vector>
#include "dll/settings.h"
#include "InGameOverlay/ImGui/imgui.h"

static constexpr int FRAMETIME_HISTORY_SIZE = 16384;

class Steam_Overlay_Stats {
private:
    class Settings* settings{};
    
    // Frame timing - high precision
    std::chrono::high_resolution_clock::time_point last_frame_timepoint =
        std::chrono::high_resolution_clock::now();
    
    // Ring buffer for frametime history (in ms, float precision)
    float frametime_history[FRAMETIME_HISTORY_SIZE]{};
    int   frametime_history_idx = 0;
    int   frametime_history_count = 0; // how many valid entries (up to FRAMETIME_HISTORY_SIZE)

    // EMA-smoothed values (updated every frame)
    float smoothed_frametime_ms = 0.0f;
    float smoothed_fps = 0.0f;

    // Min/max/avg over visible window (updated every frame)
    float min_frametime_ms = 0.0f;
    float max_frametime_ms = 0.0f;
    float avg_frametime_ms = 0.0f;

    // Display values (snapshotted every 500ms for stable text)
    std::chrono::high_resolution_clock::time_point last_display_update =
        std::chrono::high_resolution_clock::now();
    float display_fps = 0.0f;
    float display_frametime_ms = 0.0f;
    float display_min_ft = 0.0f;
    float display_max_ft = 0.0f;
    float display_avg_ft = 0.0f;

    // Playtime
    std::chrono::high_resolution_clock::time_point initial_time =
        std::chrono::high_resolution_clock::now();
    std::chrono::high_resolution_clock::time_point last_playtime =
        std::chrono::high_resolution_clock::now();
    std::chrono::high_resolution_clock::duration total_playtime_paused{0};
    std::optional<std::chrono::high_resolution_clock::time_point> playtime_pause_start;
    unsigned active_playtime_hr = 0;
    unsigned active_playtime_min = 0;
    unsigned active_playtime_sec = 0;

    void update_frametime(const std::chrono::high_resolution_clock::time_point &now);
    void update_playtime(const std::chrono::high_resolution_clock::time_point &now);

    // Returns how many ring buffer entries fit within graph_timeframe_sec
    int get_visible_frame_count() const;

public:
    ImFont *font = nullptr;
    
    // Optional colour-space transform callback.  When non-null, every PushStyleColor call
    // in the stats HUD is routed through this to match the current swapchain colour space.
    // Set by the overlay render proc each frame (may be null for SDR).
    ImVec4 (*color_transform)(ImVec4) = nullptr;

    // Master toggles
    bool show_fps = false;
    bool show_frametime = false;
    bool show_playtime = false;
    
    // Detailed settings
    bool show_fps_graph = true;
    bool show_frametime_graph = true;
    bool show_min_max_avg = true;
    bool show_percentile_1 = true;
    bool show_percentile_5 = true;
    bool show_percentile_01 = false;
    int  graph_timeframe_sec = 5;  // 1-30

    // Stats settings window
    bool show_stats_settings = false;

    Steam_Overlay_Stats(class Settings* settings);

    bool show_any_stats() const;
    void render_stats(int current_language);
    void render_stats_settings(int current_language);
};


#endif // _STEAM_OVERLAY_STATS_H_
