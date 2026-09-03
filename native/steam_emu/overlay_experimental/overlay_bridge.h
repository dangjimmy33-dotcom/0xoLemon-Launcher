/*
 * GSE Overlay Bridge — C ABI shared between steam_api DLL and ReShade addon.
 * Version 7 (clean rewrite).
 *
 * The emu DLL (steam_api.dll / steam_api64.dll) exports functions prefixed
 * with GSE_OverlayBridge_.  The ReShade addon resolves them at load time via
 * GetProcAddress and calls them each frame from the render thread.
 *
 * ALL DATA RETURNED BY BRIDGE FUNCTIONS IS ONLY VALID UNTIL THE NEXT CALL
 * TO THE SAME FUNCTION.  The addon must copy anything it needs to keep.
 */

#ifndef GSE_OVERLAY_BRIDGE_H
#define GSE_OVERLAY_BRIDGE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── ABI version ──────────────────────────────────────────────────────── */

#define GSE_BRIDGE_ABI_VERSION 15

/* ── Enums ────────────────────────────────────────────────────────────── */

enum GSE_NotifPosition {
    GSE_NOTIF_POS_TOP_LEFT     = 0,
    GSE_NOTIF_POS_TOP_RIGHT    = 1,
    GSE_NOTIF_POS_BOT_LEFT     = 2,
    GSE_NOTIF_POS_BOT_RIGHT    = 3,
    GSE_NOTIF_POS_TOP_CENTER   = 4,
    GSE_NOTIF_POS_BOT_CENTER   = 5,
};

enum GSE_NotifType {
    GSE_NOTIF_MESSAGE            = 0,
    GSE_NOTIF_INVITE             = 1,
    GSE_NOTIF_ACHIEVEMENT        = 2,
    GSE_NOTIF_ACHIEVEMENT_PROG   = 3,
    GSE_NOTIF_AUTO_ACCEPT_INVITE = 4,
    GSE_NOTIF_LOBBY_JOIN_REQ     = 5,
    GSE_NOTIF_LOBBY_JOIN_RESP    = 6,
    GSE_NOTIF_LOBBY_KICKED       = 7,
    GSE_NOTIF_FRIEND_LOBBY       = 8,
    GSE_NOTIF_LOBBY_STATUS       = 9,
};

enum GSE_RendererAPI {
    GSE_RENDERER_UNKNOWN = 0,
    GSE_RENDERER_DX9     = 1,
    GSE_RENDERER_DX10    = 2,
    GSE_RENDERER_DX11    = 3,
    GSE_RENDERER_DX12    = 4,
    GSE_RENDERER_OPENGL  = 5,
    GSE_RENDERER_VULKAN  = 6,
};

/* ── Data structs (C-compatible, layout-stable) ───────────────────────── */

typedef struct GSE_OverlayState {
    uint32_t abi_version;           /* must be GSE_BRIDGE_ABI_VERSION */
    uint32_t app_id;
    uint8_t  is_ready;              /* overlay infrastructure ready? */
    uint8_t  show_overlay;          /* main overlay visible? */
    uint8_t  warn_local_save;
    uint8_t  warn_bad_appid;
    uint8_t  renderer_api;          /* GSE_RendererAPI */
    uint8_t  notif_position;        /* GSE_NotifPosition */
    uint8_t  show_fps;
    uint8_t  show_frametime;
    uint8_t  show_playtime;
    uint8_t  show_fps_graph;
    uint8_t  show_frametime_graph;
    uint8_t  show_min_max_avg;
    uint8_t  show_percentile_1;
    uint8_t  show_percentile_5;
    uint8_t  show_percentile_01;
    uint8_t  _pad[1];
    int32_t  graph_timeframe_sec;
    float    active_fps;
    float    active_frametime_ms;
    float    active_playtime_hr;
    float    active_playtime_min;
    float    active_playtime_sec;
    char     username[256];
    char     language[64];
    char     build_string[128];
    char     build_date[64];
    uint64_t steam_id;              /* local player SteamID64 (added in ABI v8) */
    char     app_name[256];         /* resolved game name for local app_id (added in ABI v11) */
} GSE_OverlayState;

typedef struct GSE_Achievement {
    char     name[256];             /* internal schema name */
    char     title[256];            /* display name */
    char     description[512];
    uint32_t progress;
    uint32_t max_progress;
    uint32_t unlock_time;
    uint8_t  hidden;
    uint8_t  achieved;
    int16_t  group_index;           /* -1 = ungrouped/base game, >=0 = index into achievement groups */
    float    global_percent;        /*  0..100, -1 if unknown */
    float    sh_local_percent;      /*  SteamHunters community hunter %, -1 if unknown */
    int16_t  sh_points;             /*  SteamHunters rarity points, 0 if unknown */
    uint8_t  obtainability;         /*  0=normal, 1=missable, 2=deprecated/glitched, 3=online-only */
    uint8_t  _pad;
    /* Icon pixel data pointers.
     * These point into the emu's decoded icon buffers.
     * Valid only until the next call to GSE_OverlayBridge_GetAchievements.
     * If icon_pixels == NULL, the icon hasn't been decoded yet. */
    const uint8_t *icon_pixels;     /* RGBA, icon_w * icon_h * 4 bytes */
    int32_t  icon_w;
    int32_t  icon_h;
    const uint8_t *icon_gray_pixels;
    int32_t  icon_gray_w;
    int32_t  icon_gray_h;
} GSE_Achievement;

typedef struct GSE_AchievementGroup {
    char     name[128];             /* sub-group name (e.g. "Gwent"), may be empty */
    char     dlc_app_name[256];     /* DLC display name, empty = base game */
    int32_t  dlc_app_id;            /* DLC app ID, 0 = base game */
    int32_t  achievement_count;     /* number of achievements in this group */
} GSE_AchievementGroup;

typedef struct GSE_Notification {
    int32_t  id;
    uint8_t  type;                  /* GSE_NotifType */
    uint8_t  _pad[3];
    int64_t  start_time_ms;         /* steady_clock ms since epoch */
    int64_t  duration_ms;
    char     message[512];
    /* For achievement notifications: */
    char     ach_title[256];
    uint32_t ach_progress;
    uint32_t ach_max_progress;
    uint8_t  ach_achieved;
    uint8_t  _pad2[3];
    /* Achievement icon (same semantics as GSE_Achievement) */
    const uint8_t *ach_icon_pixels;
    int32_t  ach_icon_w;
    int32_t  ach_icon_h;
    /* For lobby join request notifications: */
    uint64_t join_request_lobby_id;
    uint64_t join_request_requester_id;
    /* For invite/message notifications — the friend who sent it: */
    uint64_t source_friend_id;
} GSE_Notification;

typedef struct GSE_DisplayInfo {
    char     name[128];
    char     encoding[16];
    char     gamut[24];
    char     transfer[16];
    char     range[8];
    uint8_t  hdr_supported;
    uint8_t  hdr_enabled;
    uint8_t  wide_color;
    uint8_t  force_disabled;
    int32_t  bpc;
    int32_t  sdr_white_nits;
} GSE_DisplayInfo;

/* SCE (Steam Card Exchange) download progress */
#define GSE_SCE_NUM_TYPES 14

typedef struct GSE_SceStatus {
    uint8_t  data_available;        /* 1 if SCE catalog data is populated */
    uint8_t  downloading;           /* 1 if download is in progress */
    uint8_t  _pad[2];
    uint32_t grand_downloaded;
    uint32_t grand_skipped;
    uint32_t grand_total;
    /* Per-type progress (14 types: cards, foil cards, etc.) */
    struct {
        uint32_t total;
        uint32_t current;
        uint32_t downloaded;
    } types[GSE_SCE_NUM_TYPES];
    /* Type labels */
    const char *type_labels[GSE_SCE_NUM_TYPES];
} GSE_SceStatus;

/* SCE catalog data — series and items for the browser */
typedef struct GSE_SceSeries {
    int32_t  series_number;
    char     series_name[256];
    int32_t  item_count;
} GSE_SceSeries;

typedef struct GSE_SceItem {
    char     name[256];
    int32_t  type;                      /* SceItemType enum: 0=TradingCard..13=StartupMovie */
    int32_t  series;                    /* series number */
    int32_t  slot;                      /* 1-based position (0=N/A) */
    int32_t  total;                     /* total items in set (0=N/A) */
    char     icon_url[512];             /* thumbnail URL */
    char     wallpaper_url[512];        /* full-res background URL */
    char     animated_url[512];         /* animated preview URL */
    char     static_img_url[512];       /* static thumbnail URL */
    char     video_mp4_url[512];
    char     video_webm_url[512];
    char     market_hash_name[256];
    char     price_text[64];
    char     rarity[32];
    char     emoticon_name[64];
    char     points_price[32];
    int32_t  badge_level;
    int32_t  badge_xp;
} GSE_SceItem;

#define GSE_CONNECT_STRING_SIZE 256
#define GSE_EXE_NAME_SIZE 256

typedef struct GSE_Friend {
    uint64_t steam_id;
    char     name[128];
    uint8_t  is_online;
    uint8_t  is_joinable;
    uint8_t  same_app;              /* 1 if friend is playing the same app */
    uint8_t  window_state;          /* bitmask: show, invite, join, etc. */
    uint8_t  in_lobby;              /* 1 if friend is currently in a lobby */
    uint8_t  in_my_lobby;           /* 1 if friend is in the local user's lobby */
    uint8_t  has_pending_invite;     /* 1 if this friend sent us a pending invite */
    uint8_t  _pad[1];
    uint32_t appid;                 /* friend's current app/game ID */
    uint64_t lobby_id;              /* friend's current lobby ID (0 if none) */
    char     connect_string[GSE_CONNECT_STRING_SIZE]; /* friend's connect string (may be empty) */
    char     lobby_owner_name[128]; /* lobby owner's persona name (empty if unknown) */
    int32_t  lobby_member_count;    /* number of members in friend's lobby */
    int32_t  lobby_member_limit;    /* max members allowed in friend's lobby */
    char     app_name[256];         /* resolved game name for friend's appid (may be empty) */
    char     ip_str[256];            /* detected IP addresses, comma-separated (empty if unknown) */
} GSE_Friend;

typedef struct GSE_LocalLobbyInfo {
    uint64_t lobby_id;              /* local user's current lobby ID (0 if none) */
    uint64_t lobby_owner;           /* lobby owner steam ID */
    int32_t  member_count;          /* number of members in lobby */
    int32_t  member_limit;          /* max members allowed */
    uint8_t  is_owner;              /* 1 if local user is the lobby owner */
    uint8_t  _pad[7];
    char     connect_string[GSE_CONNECT_STRING_SIZE]; /* connect string if set (may be empty) */
    char     exe_name[GSE_EXE_NAME_SIZE];             /* game executable filename (e.g. "game.exe") */
} GSE_LocalLobbyInfo;

typedef struct GSE_GameServerInfo {
    uint8_t  active;                /* 1 if a game server is running in this process */
    uint8_t  _pad[3];
    uint32_t num_players;           /* current player count */
    uint32_t max_players;           /* max player count */
    uint32_t bot_players;           /* bot count */
    char     server_name[128];      /* server name */
    char     map_name[64];          /* current map */
} GSE_GameServerInfo;

/* Chat state for a friend (for ReShade addon chat UI) */
#define GSE_CHAT_HISTORY_SIZE 4096
#define GSE_CHAT_INPUT_SIZE   768

typedef struct GSE_ChatState {
    uint64_t steam_id;                          /* friend's steam ID */
    uint8_t  is_open;                           /* 1 if chat window should be shown */
    uint8_t  has_pending_invite;                /* 1 if there's a pending invite in this chat */
    uint8_t  needs_attention;                   /* 1 if unread messages */
    uint8_t  _pad;
    char     chat_history[GSE_CHAT_HISTORY_SIZE]; /* read-only history text */
    char     window_title[128];                 /* friend name + status for window title */
} GSE_ChatState;

/* Avatar info - raw RGBA pixel data */
#define GSE_AVATAR_SIZE 64   /* 64x64 pixels */
#define GSE_AVATAR_BYTES (GSE_AVATAR_SIZE * GSE_AVATAR_SIZE * 4)

/* ── Lobby Chat ──────────────────────────────────────────────────────── */
#define GSE_LOBBY_CHAT_HISTORY_SIZE 8192
#define GSE_LOBBY_CHAT_MAX_MEMBERS  32

typedef struct GSE_LobbyChatMember {
    uint64_t steam_id;
    char     name[128];
} GSE_LobbyChatMember;

typedef struct GSE_LobbyChatState {
    uint64_t lobby_id;                              /* current lobby ID (0 if not in lobby) */
    int32_t  member_count;                          /* members in lobby */
    int32_t  history_len;                           /* bytes written to chat_history (excl NUL) */
    char     chat_history[GSE_LOBBY_CHAT_HISTORY_SIZE]; /* newline-separated chat log */
    GSE_LobbyChatMember members[GSE_LOBBY_CHAT_MAX_MEMBERS];
} GSE_LobbyChatState;

typedef struct GSE_AvatarData {
    uint64_t steam_id;                      /* owner of this avatar */
    uint8_t  valid;                         /* 1 if avatar data is valid */
    uint8_t  _pad[7];
    uint8_t  pixels[GSE_AVATAR_BYTES];      /* RGBA 64x64 = 16384 bytes */
} GSE_AvatarData;

/* ── Function pointer typedefs (for GetProcAddress) ───────────────────── */

/* Core */
typedef uint32_t  (*pfn_GSE_OverlayBridge_GetVersion)(void);
typedef int       (*pfn_GSE_OverlayBridge_GetState)(GSE_OverlayState *out);
typedef void      (*pfn_GSE_OverlayBridge_ShowOverlay)(int show);

/* Achievements */
typedef int       (*pfn_GSE_OverlayBridge_GetAchievementCount)(void);
typedef int       (*pfn_GSE_OverlayBridge_GetAchievements)(GSE_Achievement *out, int max_count);
typedef int       (*pfn_GSE_OverlayBridge_HasAchievementGroups)(void);  /* 1 if steamhunters group data is available */
typedef int       (*pfn_GSE_OverlayBridge_GetAchievementGroupCount)(void);
typedef int       (*pfn_GSE_OverlayBridge_GetAchievementGroups)(GSE_AchievementGroup *out, int max_count);

/* Notifications — returns count of active (non-expired) notifications */
typedef int       (*pfn_GSE_OverlayBridge_GetNotifications)(GSE_Notification *out, int max_count);
typedef void      (*pfn_GSE_OverlayBridge_ExpireNotification)(int id);
typedef void      (*pfn_GSE_OverlayBridge_AcceptLobbyJoinRequest)(int notification_id);
typedef void      (*pfn_GSE_OverlayBridge_DeclineLobbyJoinRequest)(int notification_id);
typedef void      (*pfn_GSE_OverlayBridge_RequestJoinFriendLobby)(uint64_t friend_id);

/* Display / HDR info */
typedef int       (*pfn_GSE_OverlayBridge_GetDisplayInfo)(GSE_DisplayInfo *out, int max_count);
typedef float     (*pfn_GSE_OverlayBridge_GetSDRWhiteScale)(void);

/* Friends */
typedef int       (*pfn_GSE_OverlayBridge_GetFriendCount)(void);
typedef int       (*pfn_GSE_OverlayBridge_GetFriends)(GSE_Friend *out, int max_count);
typedef int       (*pfn_GSE_OverlayBridge_HasLobby)(void);  /* 1 if local user has a lobby/connect string */
typedef int       (*pfn_GSE_OverlayBridge_GetLocalLobbyInfo)(GSE_LocalLobbyInfo *out);  /* returns 1 if in lobby */
typedef int       (*pfn_GSE_OverlayBridge_GetConnectString)(char *out, int out_size);  /* returns 1 if connect string set */
typedef int       (*pfn_GSE_OverlayBridge_GetExeName)(char *out, int out_size);  /* returns 1 if exe name available */
typedef int       (*pfn_GSE_OverlayBridge_GetGameServerInfo)(GSE_GameServerInfo *out);  /* returns 1 if game server active */

/* Language */
typedef int       (*pfn_GSE_OverlayBridge_GetLanguage)(void);  /* returns language index (0-30) for translations */

/* Settings read/write (key-value string pairs) */
typedef int       (*pfn_GSE_OverlayBridge_GetOption)(int option_id);
typedef void      (*pfn_GSE_OverlayBridge_SetOption)(int option_id, int value);

/* Actions (trigger emu-side behaviour) */
typedef void      (*pfn_GSE_OverlayBridge_TestAchievement)(void);
typedef void      (*pfn_GSE_OverlayBridge_ResetAchievements)(void);
typedef void      (*pfn_GSE_OverlayBridge_SimulateAchievements)(void);
typedef void      (*pfn_GSE_OverlayBridge_InviteAllFriends)(void);
typedef void      (*pfn_GSE_OverlayBridge_FriendAction)(uint64_t steam_id, int action);
typedef void      (*pfn_GSE_OverlayBridge_KickAllLobbyMembers)(void);
typedef void      (*pfn_GSE_OverlayBridge_LeaveLobby)(void);

/* SCE (Steam Card Exchange) */
typedef int       (*pfn_GSE_OverlayBridge_GetSceStatus)(GSE_SceStatus *out);
typedef void      (*pfn_GSE_OverlayBridge_RequestSceDownload)(void);
typedef int       (*pfn_GSE_OverlayBridge_GetSceSeriesCount)(void);
typedef int       (*pfn_GSE_OverlayBridge_GetSceSeries)(GSE_SceSeries *out, int max_count);
typedef int       (*pfn_GSE_OverlayBridge_GetSceItems)(int series_number, GSE_SceItem *out, int max_count);
typedef int       (*pfn_GSE_OverlayBridge_GetSceStoragePath)(char *out, int out_size);

/* Chat - for ReShade addon to render chat UI */
typedef int       (*pfn_GSE_OverlayBridge_GetChatState)(uint64_t steam_id, GSE_ChatState *out);  /* returns 1 if chat exists */
typedef void      (*pfn_GSE_OverlayBridge_SendChatMessage)(uint64_t steam_id, const char *msg);
typedef void      (*pfn_GSE_OverlayBridge_OpenChat)(uint64_t steam_id);
typedef void      (*pfn_GSE_OverlayBridge_CloseChat)(uint64_t steam_id);

/* Avatars - retrieve avatar pixel data */
typedef int       (*pfn_GSE_OverlayBridge_GetAvatar)(uint64_t steam_id, GSE_AvatarData *out);  /* returns 1 if avatar available */
typedef int       (*pfn_GSE_OverlayBridge_GetLocalAvatar)(GSE_AvatarData *out);  /* returns 1 if avatar available */

/* ── Notification appearance (all configurable values from overlay settings) ── */
typedef struct GSE_NotifAppearance {
    float icon_size;                /* achievement icon size (px), default 64 */
    float notification_rounding;    /* window corner rounding (px), default 10 */
    float notification_margin_x;    /* horizontal gap between notifications and screen edge (px), default 5 */
    float notification_margin_y;    /* vertical gap between stacked notifications (px), default 5 */
    uint32_t notification_animation;/* slide in/out animation duration (ms), default 350 */
    float notification_r;           /* background color red   (0-1), default 0.12 */
    float notification_g;           /* background color green (0-1), default 0.14 */
    float notification_b;           /* background color blue  (0-1), default 0.21 */
    float notification_a;           /* background color alpha (0-1), default 1.0  */
    float background_r;             /* main overlay background red   (0-1) */
    float background_g;             /* main overlay background green (0-1) */
    float background_b;             /* main overlay background blue  (0-1) */
    float background_a;             /* main overlay background alpha (0-1) */
    float element_r;                /* button/frame color red   (0-1) */
    float element_g;                /* button/frame color green (0-1) */
    float element_b;                /* button/frame color blue  (0-1) */
    float element_a;                /* button/frame color alpha (0-1) */
    float element_hovered_r;        /* hovered element red   (0-1) */
    float element_hovered_g;        /* hovered element green (0-1) */
    float element_hovered_b;        /* hovered element blue  (0-1) */
    float element_hovered_a;        /* hovered element alpha (0-1) */
    float stats_background_r;       /* stats HUD background red   (0-1) */
    float stats_background_g;       /* stats HUD background green (0-1) */
    float stats_background_b;       /* stats HUD background blue  (0-1) */
    float stats_background_a;       /* stats HUD background alpha (0-1) */
    float stats_text_r;             /* stats HUD text red   (0-1) */
    float stats_text_g;             /* stats HUD text green (0-1) */
    float stats_text_b;             /* stats HUD text blue  (0-1) */
    float stats_text_a;             /* stats HUD text alpha (0-1) */
    float width_percent;            /* minimum notification width as fraction of screen (0.25 = 25%) */
    int32_t swapchain_override;     /* SwapchainOverride enum: 0=Auto, 1=LinearHDR, 2=HDR10PQ, 3=SrgbRTV, 4=SDR */
    int32_t image_gamma;            /* SrgbDecode enum: 0=Auto, 1=Enabled, 2=Disabled */
    float   image_brightness;       /* linear RGB multiplier (1.0 = unchanged, 0.5-2.0) */
    float   image_contrast;         /* contrast around mid-gray (1.0 = unchanged, 0.5-2.0) */
    float   image_gamma_adjust;     /* power-curve gamma (1.0 = unchanged, 0.5-2.0) */
} GSE_NotifAppearance;

typedef int       (*pfn_GSE_OverlayBridge_GetNotifAppearance)(GSE_NotifAppearance *out);  /* returns 1 on success */
typedef int       (*pfn_GSE_OverlayBridge_GetLocalIP)(char *out, int max_len);             /* returns 1 if IP available */

/* ── Network topology info ───────────────────────────────────────────── */
#define GSE_NET_MAX_USERS_PER_ADAPTER 32

typedef struct GSE_NetUser {
    uint64_t steam_id;
    char     name[128];
    char     ip_str[24];
    uint8_t  is_self;           /* 1 if this is the local user */
    uint8_t  _pad[7];
} GSE_NetUser;

typedef struct GSE_NetAdapter {
    char     name[128];         /* adapter friendly name (e.g. "Ethernet", "Wi-Fi") */
    char     ip_str[24];        /* our IP on this adapter (dotted-quad) */
    char     subnet_str[32];    /* CIDR notation (e.g. "192.168.1.0/24") */
    char     range_str[48];     /* IP range (e.g. "192.168.1.0 - 192.168.1.255") */
    int32_t  user_count;        /* number of entries in users[] */
    int32_t  _pad;
    GSE_NetUser users[GSE_NET_MAX_USERS_PER_ADAPTER];
} GSE_NetAdapter;

typedef int       (*pfn_GSE_OverlayBridge_GetNetworkInfo)(GSE_NetAdapter *out, int max_adapters); /* returns adapter count */

/* Lobby Chat */
typedef int       (*pfn_GSE_OverlayBridge_GetLobbyChatState)(GSE_LobbyChatState *out);  /* returns 1 if in lobby */
typedef void      (*pfn_GSE_OverlayBridge_SendLobbyChatMsg)(const char *msg);

/* Friend action IDs */
#define GSE_FRIEND_ACTION_INVITE         1
#define GSE_FRIEND_ACTION_JOIN           2
#define GSE_FRIEND_ACTION_COPY_ID        3
#define GSE_FRIEND_ACTION_CHAT           4
#define GSE_FRIEND_ACTION_ACCEPT_INVITE  5
#define GSE_FRIEND_ACTION_REFUSE_INVITE  6
#define GSE_FRIEND_ACTION_KICK           7

/* Option IDs for Get/SetOption */
#define GSE_OPT_FRIEND_NOTIF_ENABLE          1  /* bool */
#define GSE_OPT_ACH_NOTIF_ENABLE             2  /* bool */
#define GSE_OPT_INVITE_NOTIF_ENABLE          3  /* bool */
#define GSE_OPT_ACH_PROGRESS_NOTIF_ENABLE    4  /* bool */
#define GSE_OPT_SORT_BY_GLOBAL_PERCENT       5  /* bool */
#define GSE_OPT_LOCAL_SAVE_WARNING           6  /* bool */
#define GSE_OPT_ALWAYS_SHOW_USER             7  /* bool */
#define GSE_OPT_ALWAYS_SHOW_STATS            8  /* bool */
#define GSE_OPT_DISABLE_ALL_WARNINGS         9  /* bool */
#define GSE_OPT_DISABLE_BAD_APPID_WARNING   10  /* bool */
#define GSE_OPT_DISABLE_LOCAL_SAVE_WARNING  11  /* bool */
#define GSE_OPT_NOTIF_POSITION              12  /* GSE_NotifPosition */
#define GSE_OPT_SHOW_FPS                    13  /* bool */
#define GSE_OPT_SHOW_FRAMETIME              14  /* bool */
#define GSE_OPT_SHOW_PLAYTIME               15  /* bool */
#define GSE_OPT_SHOW_FPS_GRAPH              19  /* bool */
#define GSE_OPT_SHOW_FRAMETIME_GRAPH        20  /* bool */
#define GSE_OPT_SHOW_MIN_MAX_AVG            21  /* bool */
#define GSE_OPT_SHOW_PERCENTILE_1           22  /* bool */
#define GSE_OPT_SHOW_PERCENTILE_5           23  /* bool */
#define GSE_OPT_SHOW_PERCENTILE_01          24  /* bool */
#define GSE_OPT_GRAPH_TIMEFRAME_SEC         25  /* int 1-30 */
#define GSE_OPT_NOTIF_POS_ACHIEVEMENT       16  /* GSE_NotifPosition — per-type override for achievement notifications */
#define GSE_OPT_NOTIF_POS_INVITE            17  /* GSE_NotifPosition — per-type override for invite/lobby join notifications */
#define GSE_OPT_NOTIF_POS_CHAT              18  /* GSE_NotifPosition — per-type override for chat message notifications */

/* ── Convenience: load all bridge functions from a module ─────────────── */

typedef struct GSE_BridgeFunctions {
    pfn_GSE_OverlayBridge_GetVersion          GetVersion;
    pfn_GSE_OverlayBridge_GetState            GetState;
    pfn_GSE_OverlayBridge_ShowOverlay         ShowOverlay;
    pfn_GSE_OverlayBridge_GetAchievementCount GetAchievementCount;
    pfn_GSE_OverlayBridge_GetAchievements     GetAchievements;
    pfn_GSE_OverlayBridge_HasAchievementGroups HasAchievementGroups;
    pfn_GSE_OverlayBridge_GetAchievementGroupCount GetAchievementGroupCount;
    pfn_GSE_OverlayBridge_GetAchievementGroups GetAchievementGroups;
    pfn_GSE_OverlayBridge_GetNotifications    GetNotifications;
    pfn_GSE_OverlayBridge_ExpireNotification  ExpireNotification;
    pfn_GSE_OverlayBridge_AcceptLobbyJoinRequest AcceptLobbyJoinRequest;
    pfn_GSE_OverlayBridge_DeclineLobbyJoinRequest DeclineLobbyJoinRequest;
    pfn_GSE_OverlayBridge_RequestJoinFriendLobby RequestJoinFriendLobby;
    pfn_GSE_OverlayBridge_GetDisplayInfo      GetDisplayInfo;
    pfn_GSE_OverlayBridge_GetSDRWhiteScale    GetSDRWhiteScale;
    pfn_GSE_OverlayBridge_GetFriendCount      GetFriendCount;
    pfn_GSE_OverlayBridge_GetFriends          GetFriends;
    pfn_GSE_OverlayBridge_HasLobby            HasLobby;
    pfn_GSE_OverlayBridge_GetLocalLobbyInfo   GetLocalLobbyInfo;
    pfn_GSE_OverlayBridge_GetConnectString    GetConnectString;
    pfn_GSE_OverlayBridge_GetExeName          GetExeName;
    pfn_GSE_OverlayBridge_GetGameServerInfo    GetGameServerInfo;
    pfn_GSE_OverlayBridge_GetLanguage         GetLanguage;
    pfn_GSE_OverlayBridge_GetOption           GetOption;
    pfn_GSE_OverlayBridge_SetOption           SetOption;
    pfn_GSE_OverlayBridge_TestAchievement     TestAchievement;
    pfn_GSE_OverlayBridge_ResetAchievements   ResetAchievements;
    pfn_GSE_OverlayBridge_SimulateAchievements SimulateAchievements;
    pfn_GSE_OverlayBridge_InviteAllFriends    InviteAllFriends;
    pfn_GSE_OverlayBridge_FriendAction        FriendAction;
    pfn_GSE_OverlayBridge_KickAllLobbyMembers KickAllLobbyMembers;
    pfn_GSE_OverlayBridge_LeaveLobby          LeaveLobby;
    pfn_GSE_OverlayBridge_GetSceStatus        GetSceStatus;
    pfn_GSE_OverlayBridge_RequestSceDownload  RequestSceDownload;
    pfn_GSE_OverlayBridge_GetSceSeriesCount   GetSceSeriesCount;
    pfn_GSE_OverlayBridge_GetSceSeries        GetSceSeries;
    pfn_GSE_OverlayBridge_GetSceItems         GetSceItems;
    pfn_GSE_OverlayBridge_GetSceStoragePath   GetSceStoragePath;
    pfn_GSE_OverlayBridge_GetChatState        GetChatState;
    pfn_GSE_OverlayBridge_SendChatMessage     SendChatMessage;
    pfn_GSE_OverlayBridge_OpenChat            OpenChat;
    pfn_GSE_OverlayBridge_CloseChat           CloseChat;
    pfn_GSE_OverlayBridge_GetAvatar           GetAvatar;
    pfn_GSE_OverlayBridge_GetLocalAvatar      GetLocalAvatar;
    pfn_GSE_OverlayBridge_GetNotifAppearance  GetNotifAppearance;
    pfn_GSE_OverlayBridge_GetLocalIP           GetLocalIP;
    pfn_GSE_OverlayBridge_GetNetworkInfo       GetNetworkInfo;
    pfn_GSE_OverlayBridge_GetLobbyChatState    GetLobbyChatState;
    pfn_GSE_OverlayBridge_SendLobbyChatMsg     SendLobbyChatMsg;
} GSE_BridgeFunctions;

#ifdef _WIN32
#include <Windows.h>
static inline int GSE_LoadBridgeFunctions(HMODULE emu_dll, GSE_BridgeFunctions *fn)
{
    if (!emu_dll || !fn) return 0;
    #define LOAD(name) fn->name = (pfn_GSE_OverlayBridge_##name)GetProcAddress(emu_dll, "GSE_OverlayBridge_" #name)
    LOAD(GetVersion);
    LOAD(GetState);
    LOAD(ShowOverlay);
    LOAD(GetAchievementCount);
    LOAD(GetAchievements);
    LOAD(HasAchievementGroups);
    LOAD(GetAchievementGroupCount);
    LOAD(GetAchievementGroups);
    LOAD(GetNotifications);
    LOAD(ExpireNotification);
    LOAD(AcceptLobbyJoinRequest);
    LOAD(DeclineLobbyJoinRequest);
    LOAD(RequestJoinFriendLobby);
    LOAD(GetDisplayInfo);
    LOAD(GetSDRWhiteScale);
    LOAD(GetFriendCount);
    LOAD(GetFriends);
    LOAD(HasLobby);
    LOAD(GetLocalLobbyInfo);
    LOAD(GetConnectString);
    LOAD(GetExeName);
    LOAD(GetGameServerInfo);
    LOAD(GetLanguage);
    LOAD(GetOption);
    LOAD(SetOption);
    LOAD(TestAchievement);
    LOAD(ResetAchievements);
    LOAD(SimulateAchievements);
    LOAD(InviteAllFriends);
    LOAD(FriendAction);
    LOAD(KickAllLobbyMembers);
    LOAD(LeaveLobby);
    LOAD(GetSceStatus);
    LOAD(RequestSceDownload);
    LOAD(GetSceSeriesCount);
    LOAD(GetSceSeries);
    LOAD(GetSceItems);
    LOAD(GetSceStoragePath);
    LOAD(GetChatState);
    LOAD(SendChatMessage);
    LOAD(OpenChat);
    LOAD(CloseChat);
    LOAD(GetAvatar);
    LOAD(GetLocalAvatar);
    LOAD(GetNotifAppearance);
    LOAD(GetLocalIP);
    LOAD(GetNetworkInfo);
    LOAD(GetLobbyChatState);
    LOAD(SendLobbyChatMsg);
    #undef LOAD
    /* At minimum, GetVersion must be present */
    return fn->GetVersion != NULL;
}
#endif

#ifdef __cplusplus
}
#endif

#endif /* GSE_OVERLAY_BRIDGE_H */
