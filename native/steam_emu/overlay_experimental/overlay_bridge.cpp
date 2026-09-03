/*
 * GSE Overlay Bridge — C ABI implementation (lives inside steam_api DLL).
 *
 * This file is compiled ONLY in the api_experimental build (EMU_OVERLAY defined).
 * It exports functions that the ReShade addon calls via GetProcAddress.
 *
 * The bridge reads from Steam_Overlay / Settings / Steam_Overlay_Stats using
 * the public accessor methods added for this purpose.
 */

#if defined(EMU_OVERLAY) && defined(_WIN32)

#ifdef _WIN32
// winsock2.h MUST come before anything that might pull in winsock.h
// (dll.h → windows.h → winsock.h) to avoid type-redefinition errors.
#include <winsock2.h>
#include <ws2tcpip.h>
#endif

#include "overlay_bridge.h"
#include "dll/dll.h"                    // get_steam_client()
#include "dll/steam_client.h"           // Steam_Client → steam_overlay
#include "overlay/steam_overlay.h"
#include "dll/steam_app_ids.h"      // steam_preowned_app_ids

#include <cstring>
#include <algorithm>

// forward-declared in steam_overlay.cpp — we need the display info query
// We'll call the public method on Steam_Overlay instead.

/* ── helper: safe string copy ─────────────────────────────────────────── */
static void safe_copy(char *dst, size_t dst_sz, const char *src)
{
    if (!dst || !dst_sz) return;
    if (!src) { dst[0] = '\0'; return; }
    strncpy(dst, src, dst_sz - 1);
    dst[dst_sz - 1] = '\0';
}

static void safe_copy(char *dst, size_t dst_sz, const std::string &src)
{
    safe_copy(dst, dst_sz, src.c_str());
}

/* ── helper: map Overlay_Appearance::NotificationPosition → GSE_NotifPosition ── */
static int bridge_map_notif_pos(int pos)
{
    // Overlay_Appearance::NotificationPosition: top_left=0, top_center=1, top_right=2, bot_left=3, bot_center=4, bot_right=5
    // GSE_NotifPosition:                        TOP_LEFT=0, TOP_RIGHT=1, BOT_LEFT=2, BOT_RIGHT=3, TOP_CENTER=4, BOT_CENTER=5
    switch (pos) {
    case 0: return GSE_NOTIF_POS_TOP_LEFT;
    case 1: return GSE_NOTIF_POS_TOP_CENTER;
    case 2: return GSE_NOTIF_POS_TOP_RIGHT;
    case 3: return GSE_NOTIF_POS_BOT_LEFT;
    case 4: return GSE_NOTIF_POS_BOT_CENTER;
    case 5: return GSE_NOTIF_POS_BOT_RIGHT;
    default: return GSE_NOTIF_POS_TOP_RIGHT;
    }
}

/* ── Exported bridge functions ────────────────────────────────────────── */

extern "C" {

__declspec(dllexport) uint32_t GSE_OverlayBridge_GetVersion(void)
{
    return GSE_BRIDGE_ABI_VERSION;
}

__declspec(dllexport) int GSE_OverlayBridge_GetState(GSE_OverlayState *out)
{
    if (!out) return 0;
    memset(out, 0, sizeof(*out));

    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;

    auto *overlay = client->steam_overlay;
    auto *settings = client->settings_client;

    // Mark that the ReShade addon is connected — suppresses native overlay rendering
    overlay->Bridge_MarkConnected();

    out->abi_version = GSE_BRIDGE_ABI_VERSION;
    out->app_id = settings ? settings->get_local_game_id().AppID() : 0;
    out->steam_id = settings ? settings->get_local_steam_id().ConvertToUint64() : 0;
    out->is_ready = overlay->Ready() ? 1 : 0;
    out->show_overlay = overlay->ShowOverlay() ? 1 : 0;
    out->warn_local_save = overlay->Bridge_GetWarnLocalSave() ? 1 : 0;
    out->warn_bad_appid = overlay->Bridge_GetWarnBadAppId() ? 1 : 0;

    // Renderer API — the addon is rendering through ReShade, so report unknown
    out->renderer_api = GSE_RENDERER_UNKNOWN;

    // Notification position
    out->notif_position = static_cast<uint8_t>(overlay->Bridge_GetNotifPosition());

    // Stats
    auto stats_state = overlay->Bridge_GetStatsState();
    out->show_fps = stats_state.show_fps ? 1 : 0;
    out->show_frametime = stats_state.show_frametime ? 1 : 0;
    out->show_playtime = stats_state.show_playtime ? 1 : 0;
    out->show_fps_graph = stats_state.show_fps_graph ? 1 : 0;
    out->show_frametime_graph = stats_state.show_frametime_graph ? 1 : 0;
    out->show_min_max_avg = stats_state.show_min_max_avg ? 1 : 0;
    out->show_percentile_1 = stats_state.show_percentile_1 ? 1 : 0;
    out->show_percentile_5 = stats_state.show_percentile_5 ? 1 : 0;
    out->show_percentile_01 = stats_state.show_percentile_01 ? 1 : 0;
    out->graph_timeframe_sec = stats_state.graph_timeframe_sec;
    out->active_fps = stats_state.fps;
    out->active_frametime_ms = stats_state.frametime_ms;
    out->active_playtime_hr = stats_state.playtime_hr;
    out->active_playtime_min = stats_state.playtime_min;
    out->active_playtime_sec = stats_state.playtime_sec;

    // Strings
    if (settings) {
        safe_copy(out->username, sizeof(out->username), settings->get_local_name());
        safe_copy(out->language, sizeof(out->language), settings->get_language());
    }

#ifndef EMU_BUILD_STRING
    #define EMU_BUILD_STRING "unknown"
#endif
#ifndef EMU_BUILD_DATE_STRING
    #define EMU_BUILD_DATE_STRING "unknown"
#endif
    safe_copy(out->build_string, sizeof(out->build_string), EMU_BUILD_STRING);
    safe_copy(out->build_date, sizeof(out->build_date), EMU_BUILD_DATE_STRING);

    // Resolve app name from preowned app IDs
    if (out->app_id != 0) {
        auto it = steam_preowned_app_ids.find(out->app_id);
        if (it != steam_preowned_app_ids.end()) {
            safe_copy(out->app_name, sizeof(out->app_name), it->second);
        }
    }

    return 1;
}

__declspec(dllexport) void GSE_OverlayBridge_ShowOverlay(int show)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->ShowOverlay(show != 0);
}

__declspec(dllexport) int GSE_OverlayBridge_GetAchievementCount(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetAchievementCount();
}

__declspec(dllexport) int GSE_OverlayBridge_GetAchievements(GSE_Achievement *out, int max_count)
{
    if (!out || max_count <= 0) return 0;

    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;

    return client->steam_overlay->Bridge_GetAchievements(out, max_count);
}

__declspec(dllexport) int GSE_OverlayBridge_HasAchievementGroups(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_HasAchievementGroups();
}

__declspec(dllexport) int GSE_OverlayBridge_GetAchievementGroupCount(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetAchievementGroupCount();
}

__declspec(dllexport) int GSE_OverlayBridge_GetAchievementGroups(GSE_AchievementGroup *out, int max_count)
{
    if (!out || max_count <= 0) return 0;

    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;

    return client->steam_overlay->Bridge_GetAchievementGroups(out, max_count);
}

__declspec(dllexport) int GSE_OverlayBridge_GetNotifications(GSE_Notification *out, int max_count)
{
    if (!out || max_count <= 0) return 0;

    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;

    return client->steam_overlay->Bridge_GetNotifications(out, max_count);
}

__declspec(dllexport) void GSE_OverlayBridge_ExpireNotification(int id)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_ExpireNotification(id);
}

__declspec(dllexport) void GSE_OverlayBridge_AcceptLobbyJoinRequest(int notification_id)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_AcceptLobbyJoinRequest(notification_id);
}

__declspec(dllexport) void GSE_OverlayBridge_DeclineLobbyJoinRequest(int notification_id)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_DeclineLobbyJoinRequest(notification_id);
}

__declspec(dllexport) void GSE_OverlayBridge_RequestJoinFriendLobby(int notification_id)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_RequestJoinFriendLobby(notification_id);
}

__declspec(dllexport) int GSE_OverlayBridge_GetDisplayInfo(GSE_DisplayInfo *out, int max_count)
{
    if (!out || max_count <= 0) return 0;

    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;

    return client->steam_overlay->Bridge_GetDisplayInfo(out, max_count);
}

__declspec(dllexport) float GSE_OverlayBridge_GetSDRWhiteScale(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 1.0f;
    return client->steam_overlay->Bridge_GetSDRWhiteScale();
}

__declspec(dllexport) int GSE_OverlayBridge_GetFriendCount(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetFriendCount();
}

__declspec(dllexport) int GSE_OverlayBridge_GetFriends(GSE_Friend *out, int max_count)
{
    if (!out || max_count <= 0) return 0;

    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;

    return client->steam_overlay->Bridge_GetFriends(out, max_count);
}

__declspec(dllexport) int GSE_OverlayBridge_HasLobby(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_HasLobby();
}

__declspec(dllexport) int GSE_OverlayBridge_GetLocalLobbyInfo(GSE_LocalLobbyInfo *out)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetLocalLobbyInfo(out);
}

__declspec(dllexport) int GSE_OverlayBridge_GetConnectString(char *out, int out_size)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetConnectString(out, out_size);
}

__declspec(dllexport) int GSE_OverlayBridge_GetExeName(char *out, int out_size)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetExeName(out, out_size);
}

__declspec(dllexport) int GSE_OverlayBridge_GetGameServerInfo(GSE_GameServerInfo *out)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetGameServerInfo(out);
}

__declspec(dllexport) int GSE_OverlayBridge_GetLanguage(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetLanguage();
}

__declspec(dllexport) int GSE_OverlayBridge_GetOption(int option_id)
{
    auto *client = get_steam_client();
    if (!client) return 0;

    auto *s = client->settings_client;
    auto *overlay = client->steam_overlay;
    if (!s) return 0;

    switch (option_id) {
    case GSE_OPT_FRIEND_NOTIF_ENABLE:          return !s->disable_overlay_friend_notification;
    case GSE_OPT_ACH_NOTIF_ENABLE:             return !s->disable_overlay_achievement_notification;
    case GSE_OPT_INVITE_NOTIF_ENABLE:          return 1; // always enabled for now
    case GSE_OPT_ACH_PROGRESS_NOTIF_ENABLE:    return !s->disable_overlay_achievement_progress;
    case GSE_OPT_SORT_BY_GLOBAL_PERCENT:       return s->overlay_achievement_sort_by_global_percent;
    case GSE_OPT_LOCAL_SAVE_WARNING:           return s->overlay_warn_local_save;
    case GSE_OPT_ALWAYS_SHOW_USER:             return s->overlay_always_show_user_info;
    case GSE_OPT_ALWAYS_SHOW_STATS:            return s->overlay_always_show_fps || s->overlay_always_show_frametime || s->overlay_always_show_playtime;
    case GSE_OPT_DISABLE_ALL_WARNINGS:         return s->disable_overlay_warning_any;
    case GSE_OPT_DISABLE_BAD_APPID_WARNING:    return s->disable_overlay_warning_bad_appid;
    case GSE_OPT_DISABLE_LOCAL_SAVE_WARNING:   return s->disable_overlay_warning_local_save;
    case GSE_OPT_NOTIF_POSITION:               return overlay ? overlay->Bridge_GetNotifPosition() : 0;
    case GSE_OPT_SHOW_FPS:                     return s->overlay_always_show_fps;
    case GSE_OPT_SHOW_FRAMETIME:               return s->overlay_always_show_frametime;
    case GSE_OPT_SHOW_PLAYTIME:                return s->overlay_always_show_playtime;
    case GSE_OPT_SHOW_FPS_GRAPH:               return s->overlay_show_fps_graph;
    case GSE_OPT_SHOW_FRAMETIME_GRAPH:         return s->overlay_show_frametime_graph;
    case GSE_OPT_SHOW_MIN_MAX_AVG:             return s->overlay_show_min_max_avg;
    case GSE_OPT_SHOW_PERCENTILE_1:            return s->overlay_show_percentile_1;
    case GSE_OPT_SHOW_PERCENTILE_5:            return s->overlay_show_percentile_5;
    case GSE_OPT_SHOW_PERCENTILE_01:           return s->overlay_show_percentile_01;
    case GSE_OPT_GRAPH_TIMEFRAME_SEC:          return s->overlay_graph_timeframe_sec;
    case GSE_OPT_NOTIF_POS_ACHIEVEMENT:        return bridge_map_notif_pos(s->overlay_appearance.ach_earned_pos);
    case GSE_OPT_NOTIF_POS_INVITE:             return bridge_map_notif_pos(s->overlay_appearance.invite_pos);
    case GSE_OPT_NOTIF_POS_CHAT:               return bridge_map_notif_pos(s->overlay_appearance.chat_msg_pos);
    default: return 0;
    }
}

__declspec(dllexport) void GSE_OverlayBridge_SetOption(int option_id, int value)
{
    auto *client = get_steam_client();
    if (!client) return;

    auto *s = client->settings_client;
    auto *overlay = client->steam_overlay;
    if (!s) return;

    switch (option_id) {
    case GSE_OPT_FRIEND_NOTIF_ENABLE:          s->disable_overlay_friend_notification = !value; break;
    case GSE_OPT_ACH_NOTIF_ENABLE:             s->disable_overlay_achievement_notification = !value; break;
    case GSE_OPT_ACH_PROGRESS_NOTIF_ENABLE:    s->disable_overlay_achievement_progress = !value; break;
    case GSE_OPT_SORT_BY_GLOBAL_PERCENT:       s->overlay_achievement_sort_by_global_percent = !!value; break;
    case GSE_OPT_LOCAL_SAVE_WARNING:           s->overlay_warn_local_save = !!value; break;
    case GSE_OPT_ALWAYS_SHOW_USER:             s->overlay_always_show_user_info = !!value; break;
    case GSE_OPT_DISABLE_ALL_WARNINGS:         s->disable_overlay_warning_any = !!value; break;
    case GSE_OPT_DISABLE_BAD_APPID_WARNING:    s->disable_overlay_warning_bad_appid = !!value; break;
    case GSE_OPT_DISABLE_LOCAL_SAVE_WARNING:   s->disable_overlay_warning_local_save = !!value; break;
    case GSE_OPT_SHOW_FPS:                     s->overlay_always_show_fps = !!value;
                                               if (overlay) overlay->Bridge_SetShowFps(!!value); break;
    case GSE_OPT_SHOW_FRAMETIME:               s->overlay_always_show_frametime = !!value;
                                               if (overlay) overlay->Bridge_SetShowFrametime(!!value); break;
    case GSE_OPT_SHOW_PLAYTIME:                s->overlay_always_show_playtime = !!value;
                                               if (overlay) overlay->Bridge_SetShowPlaytime(!!value); break;
    case GSE_OPT_SHOW_FPS_GRAPH:               s->overlay_show_fps_graph = !!value;
                                               if (overlay) overlay->Bridge_SetShowFpsGraph(!!value); break;
    case GSE_OPT_SHOW_FRAMETIME_GRAPH:         s->overlay_show_frametime_graph = !!value;
                                               if (overlay) overlay->Bridge_SetShowFrametimeGraph(!!value); break;
    case GSE_OPT_SHOW_MIN_MAX_AVG:             s->overlay_show_min_max_avg = !!value;
                                               if (overlay) overlay->Bridge_SetShowMinMaxAvg(!!value); break;
    case GSE_OPT_SHOW_PERCENTILE_1:            s->overlay_show_percentile_1 = !!value;
                                               if (overlay) overlay->Bridge_SetShowPercentile1(!!value); break;
    case GSE_OPT_SHOW_PERCENTILE_5:            s->overlay_show_percentile_5 = !!value;
                                               if (overlay) overlay->Bridge_SetShowPercentile5(!!value); break;
    case GSE_OPT_SHOW_PERCENTILE_01:           s->overlay_show_percentile_01 = !!value;
                                               if (overlay) overlay->Bridge_SetShowPercentile01(!!value); break;
    case GSE_OPT_GRAPH_TIMEFRAME_SEC:          { int v = value; if (v >= 1 && v <= 30) { s->overlay_graph_timeframe_sec = v;
                                               if (overlay) overlay->Bridge_SetGraphTimeframe(v); } break; }
    default: break;
    }

    // Trigger settings save
    if (client->steam_overlay) {
        client->steam_overlay->Bridge_RequestSaveSettings();
    }
}

__declspec(dllexport) void GSE_OverlayBridge_TestAchievement(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_TestAchievement();
}

__declspec(dllexport) void GSE_OverlayBridge_ResetAchievements(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_ResetAchievements();
}

__declspec(dllexport) void GSE_OverlayBridge_SimulateAchievements(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_SimulateAchievements();
}

__declspec(dllexport) void GSE_OverlayBridge_InviteAllFriends(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_InviteAllFriends();
}

__declspec(dllexport) void GSE_OverlayBridge_FriendAction(uint64_t steam_id, int action)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_FriendAction(steam_id, action);
}

__declspec(dllexport) void GSE_OverlayBridge_KickAllLobbyMembers(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_KickAllLobbyMembers();
}

__declspec(dllexport) void GSE_OverlayBridge_LeaveLobby(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_LeaveLobby();
}

__declspec(dllexport) int GSE_OverlayBridge_GetSceStatus(GSE_SceStatus *out)
{
    if (!out) return 0;
    memset(out, 0, sizeof(*out));

    auto *client = get_steam_client();
    if (!client || !client->steam_user_stats) return 0;

    auto *us = client->steam_user_stats;
    out->data_available = (us->sce_data_populated && !us->sce_game_data.series.empty()) ? 1 : 0;
    out->downloading = us->sce_assets_downloading.load() ? 1 : 0;
    out->grand_downloaded = us->sce_assets_downloaded.load();
    out->grand_skipped = us->sce_assets_skipped.load();
    out->grand_total = us->sce_assets_total.load();

    for (int i = 0; i < GSE_SCE_NUM_TYPES && i < 14; ++i) {
        out->types[i].total = us->sce_type_progress[i].total.load();
        out->types[i].current = us->sce_type_progress[i].current.load();
        out->types[i].downloaded = us->sce_type_progress[i].downloaded.load();
        out->type_labels[i] = us->SCE_TYPE_LABELS[i];
    }

    return 1;
}

__declspec(dllexport) void GSE_OverlayBridge_RequestSceDownload(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_user_stats) return;
    client->steam_user_stats->RequestSceAssetDownload();
}

__declspec(dllexport) int GSE_OverlayBridge_GetSceSeriesCount(void)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_user_stats) return 0;
    auto *us = client->steam_user_stats;
    if (!us->sce_data_populated) return 0;
    return (int)us->sce_game_data.series.size();
}

__declspec(dllexport) int GSE_OverlayBridge_GetSceSeries(GSE_SceSeries *out, int max_count)
{
    if (!out || max_count <= 0) return 0;
    auto *client = get_steam_client();
    if (!client || !client->steam_user_stats) return 0;
    auto *us = client->steam_user_stats;
    if (!us->sce_data_populated) return 0;

    int count = (std::min)(max_count, (int)us->sce_game_data.series.size());
    for (int i = 0; i < count; ++i) {
        auto &s = us->sce_game_data.series[i];
        memset(&out[i], 0, sizeof(out[i]));
        out[i].series_number = s.series_number;
        safe_copy(out[i].series_name, sizeof(out[i].series_name), s.series_name.c_str());
        out[i].item_count = (int32_t)s.items.size();
    }
    return count;
}

__declspec(dllexport) int GSE_OverlayBridge_GetSceItems(int series_number, GSE_SceItem *out, int max_count)
{
    if (!out || max_count <= 0) return 0;
    auto *client = get_steam_client();
    if (!client || !client->steam_user_stats) return 0;
    auto *us = client->steam_user_stats;
    if (!us->sce_data_populated) return 0;

    // Find the series
    const decltype(us->sce_game_data.series)::value_type *series = nullptr;
    for (auto &s : us->sce_game_data.series) {
        if (s.series_number == series_number) { series = &s; break; }
    }
    if (!series) return 0;

    int count = (std::min)(max_count, (int)series->items.size());
    for (int i = 0; i < count; ++i) {
        auto &item = series->items[i];
        memset(&out[i], 0, sizeof(out[i]));
        safe_copy(out[i].name, sizeof(out[i].name), item.name.c_str());
        out[i].type = (int32_t)item.type;
        out[i].series = item.series;
        out[i].slot = item.slot;
        out[i].total = item.total;
        safe_copy(out[i].icon_url, sizeof(out[i].icon_url), item.icon_url.c_str());
        safe_copy(out[i].wallpaper_url, sizeof(out[i].wallpaper_url), item.wallpaper_url.c_str());
        safe_copy(out[i].animated_url, sizeof(out[i].animated_url), item.animated_url.c_str());
        safe_copy(out[i].static_img_url, sizeof(out[i].static_img_url), item.static_img_url.c_str());
        safe_copy(out[i].video_mp4_url, sizeof(out[i].video_mp4_url), item.video_mp4_url.c_str());
        safe_copy(out[i].video_webm_url, sizeof(out[i].video_webm_url), item.video_webm_url.c_str());
        safe_copy(out[i].market_hash_name, sizeof(out[i].market_hash_name), item.market_hash_name.c_str());
        safe_copy(out[i].price_text, sizeof(out[i].price_text), item.price_text.c_str());
        safe_copy(out[i].rarity, sizeof(out[i].rarity), item.rarity.c_str());
        safe_copy(out[i].emoticon_name, sizeof(out[i].emoticon_name), item.emoticon_name.c_str());
        safe_copy(out[i].points_price, sizeof(out[i].points_price), item.points_price.c_str());
        out[i].badge_level = item.badge_level;
        out[i].badge_xp = item.badge_xp;
    }
    return count;
}

__declspec(dllexport) int GSE_OverlayBridge_GetSceStoragePath(char *out, int out_size)
{
    if (!out || out_size <= 0) return 0;
    auto *client = get_steam_client();
    if (!client || !client->local_storage) return 0;
    // get_path returns save_directory + appid + folder and creates it
    std::string path = client->local_storage->get_path(
        std::string(PATH_SEPARATOR) + Steam_User_Stats::sce_assets_folder + PATH_SEPARATOR);
    safe_copy(out, out_size, path.c_str());
    return (int)path.size();
}

/* ── Chat functions ───────────────────────────────────────────────────── */

__declspec(dllexport) int GSE_OverlayBridge_GetChatState(uint64_t steam_id, GSE_ChatState *out)
{
    if (!out) return 0;
    memset(out, 0, sizeof(*out));
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetChatState(steam_id, out);
}

__declspec(dllexport) void GSE_OverlayBridge_SendChatMessage(uint64_t steam_id, const char *msg)
{
    if (!msg || !msg[0]) return;
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_SendChatMessage(steam_id, msg);
}

__declspec(dllexport) void GSE_OverlayBridge_OpenChat(uint64_t steam_id)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_OpenChat(steam_id);
}

__declspec(dllexport) void GSE_OverlayBridge_CloseChat(uint64_t steam_id)
{
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_CloseChat(steam_id);
}

/* ── Avatar functions ─────────────────────────────────────────────────── */

__declspec(dllexport) int GSE_OverlayBridge_GetAvatar(uint64_t steam_id, GSE_AvatarData *out)
{
    if (!out) return 0;
    memset(out, 0, sizeof(*out));
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetAvatar(steam_id, out);
}

__declspec(dllexport) int GSE_OverlayBridge_GetLocalAvatar(GSE_AvatarData *out)
{
    if (!out) return 0;
    memset(out, 0, sizeof(*out));
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetLocalAvatar(out);
}

__declspec(dllexport) int GSE_OverlayBridge_GetNotifAppearance(GSE_NotifAppearance *out)
{
    if (!out) return 0;
    memset(out, 0, sizeof(*out));
    auto *client = get_steam_client();
    if (!client) return 0;
    auto *s = client->settings_client;
    if (!s) return 0;
    auto &a = s->overlay_appearance;
    out->icon_size              = a.icon_size;
    out->notification_rounding  = a.notification_rounding;
    out->notification_margin_x  = a.notification_margin_x;
    out->notification_margin_y  = a.notification_margin_y;
    out->notification_animation = a.notification_animation;
    out->notification_r         = a.notification_r;
    out->notification_g         = a.notification_g;
    out->notification_b         = a.notification_b;
    out->notification_a         = a.notification_a;
    out->background_r           = a.background_r;
    out->background_g           = a.background_g;
    out->background_b           = a.background_b;
    out->background_a           = a.background_a;
    out->element_r              = a.element_r;
    out->element_g              = a.element_g;
    out->element_b              = a.element_b;
    out->element_a              = a.element_a;
    out->element_hovered_r      = a.element_hovered_r;
    out->element_hovered_g      = a.element_hovered_g;
    out->element_hovered_b      = a.element_hovered_b;
    out->element_hovered_a      = a.element_hovered_a;
    out->stats_background_r     = a.stats_background_r;
    out->stats_background_g     = a.stats_background_g;
    out->stats_background_b     = a.stats_background_b;
    out->stats_background_a     = a.stats_background_a;
    out->stats_text_r           = a.stats_text_r;
    out->stats_text_g           = a.stats_text_g;
    out->stats_text_b           = a.stats_text_b;
    out->stats_text_a           = a.stats_text_a;
    out->width_percent          = Notification::width_percent;
    out->swapchain_override     = static_cast<int32_t>(a.swapchain_override);
    out->image_gamma            = static_cast<int32_t>(a.image_gamma);
    out->image_brightness       = a.image_brightness;
    out->image_contrast         = a.image_contrast;
    out->image_gamma_adjust     = a.image_gamma_adjust;
    return 1;
}

__declspec(dllexport) int GSE_OverlayBridge_GetLocalIP(char *out, int max_len)
{
    if (!out || max_len <= 0) return 0;
    out[0] = '\0';
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetLocalIP(out, max_len);
}

__declspec(dllexport) int GSE_OverlayBridge_GetNetworkInfo(GSE_NetAdapter *out, int max_adapters)
{
    if (!out || max_adapters <= 0) return 0;
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetNetworkInfo(out, max_adapters);
}

__declspec(dllexport) int GSE_OverlayBridge_GetLobbyChatState(GSE_LobbyChatState *out)
{
    if (!out) return 0;
    memset(out, 0, sizeof(*out));
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return 0;
    return client->steam_overlay->Bridge_GetLobbyChatState(out);
}

__declspec(dllexport) void GSE_OverlayBridge_SendLobbyChatMsg(const char *msg)
{
    if (!msg || !msg[0]) return;
    auto *client = get_steam_client();
    if (!client || !client->steam_overlay) return;
    client->steam_overlay->Bridge_SendLobbyChatMsg(msg);
}

} // extern "C"

#endif // EMU_OVERLAY
