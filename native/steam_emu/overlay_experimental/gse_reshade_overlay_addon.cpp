/*
 * GSE Overlay — ReShade Addon
 *
 * This addon replaces ingame_overlay's rendering when the user has ReShade
 * installed.  It communicates with the emu DLL (steam_api.dll) through the
 * GSE_OverlayBridge C ABI, and renders through ReShade's ImGui integration.
 *
 * Build as a separate DLL with .addon64 / .addon extension.
 * Place in the game directory alongside ReShade's dxgi.dll.
 *
 * Architecture:
 *   steam_api.dll  ──(exports GSE_OverlayBridge_*)──>  gse_overlay.addon
 *        │                                                    │
 *        │  achievement/friend/notification data              │  ImGui rendering
 *        │  flows through C ABI structs                       │  via ReShade overlay
 *        └────────────────────────────────────────────────────┘
 */

#define ImTextureID ImU64
#include <imgui.h>
#include <reshade.hpp>

#include "overlay_bridge.h"
#include "overlay/steam_overlay_translations.h"

#include <cstring>
#include <cstdio>
#include <cmath>
#include <chrono>
#include <vector>
#include <unordered_map>
#include <unordered_set>
#include <mutex>
#include <algorithm>
#include <numeric>
#include <string>

#define STB_IMAGE_IMPLEMENTATION
#define STBI_ONLY_PNG
#define STBI_ONLY_JPEG
#define STBI_ONLY_GIF
#include "../libs/stb/stb_image.h"

using namespace reshade::api;

/* ── Addon metadata (required exports) ────────────────────────────────── */

extern "C" __declspec(dllexport) const char *NAME        = "GSE Overlay";
extern "C" __declspec(dllexport) const char *DESCRIPTION = "Steam emu overlay rendered through ReShade";

/* ── Bridge connection state ──────────────────────────────────────────── */

static HMODULE          s_emu_dll  = nullptr;
static GSE_BridgeFunctions s_bridge  = {};
static bool             s_bridge_ok = false;
static int              s_current_language = 0;  // translation language index

/* ── Per-device GPU texture cache (achievement icons) ─────────────────── */

struct IconTexture {
    resource     tex  = {};
    resource_view srv = {};
    int          w    = 0;
    int          h    = 0;
    bool         valid = false;
};

struct __declspec(uuid("a1b2c3d4-1234-5678-abcd-ef0123456789")) addon_device_data
{
    std::unordered_map<std::string, IconTexture> icon_cache;   // key = ach name + "_achieved"/"_locked"
    std::unordered_map<uint64_t, IconTexture>    avatar_cache; // key = steam_id
    std::vector<IconTexture> pending_destroy;                  // deferred GPU resource destruction
};

/* ── Forward declarations ──────────────────────────────────────────────── */

static void render_main_overlay(effect_runtime *runtime);
static void render_achievement_list();
static void render_friends_list();
static void render_chat_windows();
static void check_incoming_messages();

/* ── Overlay toggle state ─────────────────────────────────────────────── */

static bool s_show_main_overlay = false;
static bool s_reshade_menu_open = false;  // true when ReShade's own overlay is open
static bool s_overlay_hidden_by_reshade = false;  // true if we hid our overlay because ReShade menu opened
static bool s_show_achievements = false;
static bool s_show_settings     = false;
static bool s_show_user_info    = false;
static bool s_show_friends      = false;
static bool s_show_networks     = false;
static bool s_show_lobby_chat   = false;
static bool s_show_sce_browser  = false;
static bool s_show_chat         = false;
static int  s_active_chat_idx   = 0;  // currently selected chat tab

/* ── SCE browser state ────────────────────────────────────────────────── */

static std::string s_sce_storage_path;     // cached base path for sce_assets/
static std::string s_sce_preview_key;      // full file path of currently previewed image
static std::vector<std::string> s_sce_preview_nav_keys;  // sibling keys for prev/next
static int s_sce_tex_per_frame = 0;        // per-frame texture load cap counter
static constexpr int MAX_SCE_TEX_PER_FRAME = 4;

/* ── Chat window state (for ReShade addon chat UI) ───────────────────── */

struct AddonChatWindow {
    uint64_t steam_id = 0;
    char friend_name[64] = {};
    char input_buf[GSE_CHAT_INPUT_SIZE] = {};
    bool scroll_to_bottom = true;
};
static std::vector<AddonChatWindow> s_open_chats;  // currently open chat windows in addon

/* ── Color constants ─────────────────────────────────────────────────── */
// Configurable colors (COL_NOTIF_BG, COL_MAIN_BG, COL_ELEMENT, etc.)
// are now macros defined after s_appearance below.

// Achievement status colors
static const ImU32  COL_ACH_ACHIEVED    = IM_COL32(0, 220, 0, 255);     // Green checkmark
static const ImU32  COL_ACH_PROGRESS    = IM_COL32(255, 180, 0, 255);   // Orange arrow
static const ImU32  COL_ACH_LOCKED      = IM_COL32(220, 0, 0, 255);     // Red X
// DLC group header colors
static const ImVec4 COL_DLC_HEADER         = ImVec4(0.20f, 0.30f, 0.45f, 0.80f);
static const ImVec4 COL_DLC_HEADER_HOVER   = ImVec4(0.25f, 0.38f, 0.55f, 0.90f);
static const ImVec4 COL_DLC_HEADER_ACTIVE  = ImVec4(0.30f, 0.45f, 0.65f, 1.00f);

// Cached appearance values (updated each frame from bridge)
static GSE_NotifAppearance s_appearance = {
    /* icon_size */             64.0f,
    /* notification_rounding */ 10.0f,
    /* notification_margin_x */ 5.0f,
    /* notification_margin_y */ 5.0f,
    /* notification_animation*/ 350,
    /* notification_r */        0.12f,
    /* notification_g */        0.14f,
    /* notification_b */        0.21f,
    /* notification_a */        1.0f,
    /* background_r */          0.12f,
    /* background_g */          0.11f,
    /* background_b */          0.11f,
    /* background_a */          0.55f,
    /* element_r */             0.30f,
    /* element_g */             0.32f,
    /* element_b */             0.40f,
    /* element_a */             1.0f,
    /* element_hovered_r */     0.278f,
    /* element_hovered_g */     0.393f,
    /* element_hovered_b */     0.602f,
    /* element_hovered_a */     1.0f,
    /* stats_background_r */    0.0f,
    /* stats_background_g */    0.0f,
    /* stats_background_b */    0.0f,
    /* stats_background_a */    0.6f,
    /* stats_text_r */          0.8f,
    /* stats_text_g */          0.7f,
    /* stats_text_b */          0.0f,
    /* stats_text_a */          1.0f,
    /* width_percent */         0.25f,
    /* swapchain_override */    0,
    /* image_gamma */           0,
    /* image_brightness */      1.0f,
    /* image_contrast */        1.0f,
    /* image_gamma_adjust */    1.0f,
};

// ── Appearance macros (read live from s_appearance each frame) ────────── //
#define COL_NOTIF_BG      ImVec4(s_appearance.notification_r, s_appearance.notification_g, s_appearance.notification_b, s_appearance.notification_a)
#define COL_MAIN_BG       ImVec4(s_appearance.background_r, s_appearance.background_g, s_appearance.background_b, s_appearance.background_a)
#define COL_ELEMENT       ImVec4(s_appearance.element_r, s_appearance.element_g, s_appearance.element_b, s_appearance.element_a)
#define COL_ELEMENT_HOVER ImVec4(s_appearance.element_hovered_r, s_appearance.element_hovered_g, s_appearance.element_hovered_b, s_appearance.element_hovered_a)
#define COL_STATS_BG      ImVec4(s_appearance.stats_background_r, s_appearance.stats_background_g, s_appearance.stats_background_b, s_appearance.stats_background_a)
#define COL_STATS_TEXT    ImVec4(s_appearance.stats_text_r, s_appearance.stats_text_g, s_appearance.stats_text_b, s_appearance.stats_text_a)
#define ICON_SIZE         (s_appearance.icon_size)
#define NOTIF_ROUNDING    (s_appearance.notification_rounding)
#define NOTIF_WIDTH_FRAC  (s_appearance.width_percent)
#define NOTIF_MARGIN_X    (s_appearance.notification_margin_x)
#define NOTIF_MARGIN_Y    (s_appearance.notification_margin_y)
#define ANIM_DURATION_MS  ((int64_t)s_appearance.notification_animation)

/* ── Current device / swapchain pointers ──────────────────────────────── */

static device    *s_current_device    = nullptr;
static swapchain *s_current_swapchain = nullptr;

/* ── Swapchain colour-space classification (mirrors native overlay) ───── */
enum SwapchainColorSpace {
    SCS_UNKNOWN     = -1,
    SCS_LINEAR_HDR  =  0,  // FP16/FP32/16-bit UNORM  (scRGB / linear)
    SCS_SDR_UNORM   =  1,  // Standard SDR 8-bit UNORM
    SCS_HDR10_PQ    =  2,  // R10G10B10A2 with PQ (ST.2084) transfer
    SCS_SDR_SRGB_RTV =  3, // _SRGB render-target view (hw sRGB encode)
};
static SwapchainColorSpace s_addon_cs        = SCS_UNKNOWN;   // auto-detected
static SwapchainColorSpace s_addon_ecs       = SCS_UNKNOWN;   // effective (after overrides)
static float               s_addon_sdr_scale = 1.0f;

// Resolve effective colour space from auto-detected + user overrides in the bridge appearance.
static SwapchainColorSpace effective_addon_cs()
{
    // Image_Gamma=off disables all transforms (legacy escape hatch)
    if (s_appearance.image_gamma == 2) // SrgbDecode::Disabled
        return SCS_SDR_UNORM;

    // Swapchain_Override takes precedence over auto-detection
    switch (s_appearance.swapchain_override) {
        case 1: return SCS_LINEAR_HDR;   // LinearHDR
        case 2: return SCS_HDR10_PQ;     // HDR10PQ
        case 3: return SCS_SDR_SRGB_RTV; // SrgbRTV
        case 4: return SCS_SDR_UNORM;    // SDR
        default: break; // Auto (0) — fall through
    }

    // Image_Gamma=on forces linear decode (legacy compat)
    if (s_appearance.image_gamma == 1) // SrgbDecode::Enabled
        return (s_addon_cs == SCS_UNKNOWN) ? SCS_LINEAR_HDR : s_addon_cs;

    return s_addon_cs; // Auto — use detected
}

/* ── FPS tracking (addon-side) ────────────────────────────────────────── */

static constexpr int ADDON_FT_HISTORY_SIZE = 16384;
static constexpr float ADDON_EMA_ALPHA = 0.1f;

static float s_addon_fps = 0.0f;
static float s_addon_frametime = 0.0f;
static float s_ft_history[ADDON_FT_HISTORY_SIZE]{};
static int   s_ft_history_idx = 0;
static int   s_ft_history_count = 0;
static float s_ft_min = 0.0f;
static float s_ft_max = 0.0f;
static float s_ft_avg = 0.0f;
static float s_display_fps = 0.0f;
static float s_display_frametime = 0.0f;
static float s_display_min_ft = 0.0f;
static float s_display_max_ft = 0.0f;
static float s_display_avg_ft = 0.0f;
static std::chrono::steady_clock::time_point s_last_frame_time = std::chrono::steady_clock::now();
static std::chrono::steady_clock::time_point s_last_display_update = std::chrono::steady_clock::now();

static int get_addon_visible_count(int timeframe_sec)
{
    if (s_ft_history_count <= 0) return 0;
    float budget_ms = timeframe_sec * 1000.0f;
    float accum = 0.0f;
    int n = 0;
    for (int i = 0; i < s_ft_history_count; i++) {
        int idx = (s_ft_history_idx - 1 - i + ADDON_FT_HISTORY_SIZE) % ADDON_FT_HISTORY_SIZE;
        accum += s_ft_history[idx];
        n++;
        if (accum >= budget_ms) break;
    }
    return n;
}

static void update_fps(int timeframe_sec)
{
    auto now = std::chrono::steady_clock::now();
    float dt_ms = std::chrono::duration<float, std::milli>(now - s_last_frame_time).count();
    s_last_frame_time = now;

    // Clamp absurd values
    if (dt_ms < 0.0f) dt_ms = 0.0f;
    if (dt_ms > 1000.0f) dt_ms = 1000.0f;

    // Ring buffer
    s_ft_history[s_ft_history_idx] = dt_ms;
    s_ft_history_idx = (s_ft_history_idx + 1) % ADDON_FT_HISTORY_SIZE;
    if (s_ft_history_count < ADDON_FT_HISTORY_SIZE) s_ft_history_count++;

    // EMA smoothing
    if (s_addon_frametime <= 0.0f) {
        s_addon_frametime = dt_ms;
    } else {
        s_addon_frametime = ADDON_EMA_ALPHA * dt_ms + (1.0f - ADDON_EMA_ALPHA) * s_addon_frametime;
    }
    s_addon_fps = (s_addon_frametime > 0.0f) ? (1000.0f / s_addon_frametime) : 0.0f;

    // Min/max/avg over visible window
    int vis = get_addon_visible_count(timeframe_sec);
    float sum = 0.0f, mn = 1e9f, mx = 0.0f;
    int ring_start = (s_ft_history_idx - vis + ADDON_FT_HISTORY_SIZE) % ADDON_FT_HISTORY_SIZE;
    for (int i = 0; i < vis; i++) {
        float v = s_ft_history[(ring_start + i) % ADDON_FT_HISTORY_SIZE];
        sum += v;
        if (v < mn) mn = v;
        if (v > mx) mx = v;
    }
    if (vis > 0) {
        s_ft_avg = sum / vis;
        s_ft_min = mn;
        s_ft_max = mx;
    }

    // Snapshot display values every 500ms for stable text
    auto display_elapsed = std::chrono::duration<float, std::milli>(now - s_last_display_update).count();
    if (display_elapsed >= 500.0f || s_display_fps <= 0.0f) {
        s_last_display_update = now;
        s_display_fps = s_addon_fps;
        s_display_frametime = s_addon_frametime;
        s_display_min_ft = s_ft_min;
        s_display_max_ft = s_ft_max;
        s_display_avg_ft = s_ft_avg;
    }
}

/* ── Playtime tracking (addon-side) ───────────────────────────────────── */

static std::chrono::steady_clock::time_point s_start_time = std::chrono::steady_clock::now();

static void get_playtime(int &hr, int &min, int &sec)
{
    auto elapsed = (int)std::chrono::duration_cast<std::chrono::seconds>(
        std::chrono::steady_clock::now() - s_start_time).count();
    hr  = elapsed / 3600;
    min = (elapsed % 3600) / 60;
    sec = elapsed % 60;
}

/* ── Swap chain format detection ──────────────────────────────────────── */

struct SwapChainInfo {
    const char* format_str;  // e.g. "R16G16B16A16_FLOAT"
    const char* type_str;    // e.g. "HDR  |  scRGB / Linear  |  BT.709+"
    const char* api_str;     // e.g. "Direct3D 11"
    SwapchainColorSpace cs;  // colour-space classification
};

static SwapChainInfo get_swapchain_info(effect_runtime *runtime)
{
    SwapChainInfo info = { "Unknown", "Unknown", "Unknown", SCS_UNKNOWN };
    
    if (!runtime) return info;
    
    device *dev = runtime->get_device();
    if (!dev) return info;
    
    // Get API name
    switch (dev->get_api()) {
        case device_api::d3d9:   info.api_str = "Direct3D 9";  break;
        case device_api::d3d10:  info.api_str = "Direct3D 10"; break;
        case device_api::d3d11:  info.api_str = "Direct3D 11"; break;
        case device_api::d3d12:  info.api_str = "Direct3D 12"; break;
        case device_api::opengl: info.api_str = "OpenGL";      break;
        case device_api::vulkan: info.api_str = "Vulkan";      break;
        default:                 info.api_str = "Unknown API"; break;
    }
    
    resource backbuffer = runtime->get_back_buffer(0);
    if (!backbuffer.handle) return info;
    
    resource_desc desc = dev->get_resource_desc(backbuffer);
    format fmt = desc.texture.format;

    // Query the actual colour space from the swap chain — this is the DXGI
    // colour space the game configured via SetColorSpace1().  Crucial for
    // R10G10B10A2 which can be either PQ (HDR10) or sRGB (display-managed HDR).
    color_space cs = s_current_swapchain ? s_current_swapchain->get_color_space()
                                            : color_space::unknown;
    
    switch (fmt) {
        // ---- Linear HDR formats (scRGB / FP / wide UNORM) ------------------
        case format::r16g16b16a16_float:
            info.format_str = "R16G16B16A16_FLOAT";
            info.type_str   = "HDR  |  scRGB / Linear  |  BT.709+";
            info.cs         = SCS_LINEAR_HDR;
            break;
        case format::r16g16b16a16_unorm:
            info.format_str = "R16G16B16A16_UNORM";
            info.type_str   = "HDR  |  Linear UNORM  |  BT.2020";
            info.cs         = SCS_LINEAR_HDR;
            break;
        case format::r32g32b32a32_float:
            info.format_str = "R32G32B32A32_FLOAT";
            info.type_str   = "HDR  |  Linear FP32  |  wide gamut";
            info.cs         = SCS_LINEAR_HDR;
            break;
        // ---- 10-bit formats (PQ or SDR — determined by actual colour space) --
        case format::r10g10b10a2_unorm:
            info.format_str = "R10G10B10A2_UNORM";
            if (cs == color_space::hdr10_pq || cs == color_space::hdr10_hlg) {
                info.type_str = "HDR10  |  PQ (ST.2084)  |  BT.2020";
                info.cs       = SCS_HDR10_PQ;
            } else if (cs == color_space::scrgb) {
                info.type_str = "HDR  |  scRGB / Linear  |  BT.709+";
                info.cs       = SCS_LINEAR_HDR;
            } else {
                info.type_str = "SDR  |  sRGB  |  10-bit (display-managed HDR)";
                info.cs       = SCS_SDR_UNORM;
            }
            break;
        case format::b10g10r10a2_unorm:
            info.format_str = "B10G10R10A2_UNORM";
            if (cs == color_space::hdr10_pq || cs == color_space::hdr10_hlg) {
                info.type_str = "HDR10  |  PQ (ST.2084)  |  BT.2020";
                info.cs       = SCS_HDR10_PQ;
            } else if (cs == color_space::scrgb) {
                info.type_str = "HDR  |  scRGB / Linear  |  BT.709+";
                info.cs       = SCS_LINEAR_HDR;
            } else {
                info.type_str = "SDR  |  sRGB  |  10-bit (display-managed HDR)";
                info.cs       = SCS_SDR_UNORM;
            }
            break;
        // ---- 8-bit SDR UNORM formats ----------------------------------------
        case format::r8g8b8a8_unorm:
            info.format_str = "R8G8B8A8_UNORM";
            info.type_str   = "SDR  |  sRGB  |  Rec.709";
            info.cs         = SCS_SDR_UNORM;
            break;
        case format::b8g8r8a8_unorm:
            info.format_str = "B8G8R8A8_UNORM";
            info.type_str   = "SDR  |  sRGB  |  Rec.709";
            info.cs         = SCS_SDR_UNORM;
            break;
        case format::b8g8r8x8_unorm:
            info.format_str = "B8G8R8X8_UNORM";
            info.type_str   = "SDR  |  sRGB  |  Rec.709  (no alpha)";
            info.cs         = SCS_SDR_UNORM;
            break;
        // ---- 8-bit _SRGB render-target views --------------------------------
        case format::r8g8b8a8_unorm_srgb:
            info.format_str = "R8G8B8A8_UNORM_SRGB";
            info.type_str   = "SDR  |  sRGB (hw encode RTV)  |  Rec.709";
            info.cs         = SCS_SDR_SRGB_RTV;
            break;
        case format::b8g8r8a8_unorm_srgb:
            info.format_str = "B8G8R8A8_UNORM_SRGB";
            info.type_str   = "SDR  |  sRGB (hw encode RTV)  |  Rec.709";
            info.cs         = SCS_SDR_SRGB_RTV;
            break;
        case format::b8g8r8x8_unorm_srgb:
            info.format_str = "B8G8R8X8_UNORM_SRGB";
            info.type_str   = "SDR  |  sRGB (hw encode RTV)  |  Rec.709  (no alpha)";
            info.cs         = SCS_SDR_SRGB_RTV;
            break;
        // ---- 16-bit SDR formats (legacy) ------------------------------------
        case format::b5g6r5_unorm:
            info.format_str = "B5G6R5_UNORM";
            info.type_str   = "SDR  |  sRGB  |  16-bit RGB565";
            info.cs         = SCS_SDR_UNORM;
            break;
        case format::b5g5r5a1_unorm:
            info.format_str = "B5G5R5A1_UNORM";
            info.type_str   = "SDR  |  sRGB  |  16-bit 5551";
            info.cs         = SCS_SDR_UNORM;
            break;
        default:
            // Try to provide some info based on format value
            {
                static char fmt_buf[32];
                snprintf(fmt_buf, sizeof(fmt_buf), "Format %u", (unsigned)fmt);
                info.format_str = fmt_buf;
                info.type_str   = "Unknown format type";
            }
            break;
    }
    
    return info;
}

/* ── Colour-space transform helpers (mirrors native overlay logic) ────── */

// sRGB EOTF: single channel 0-1 sRGB → linear
static inline float srgb_to_linear(float s)
{
    return (s <= 0.04045f) ? s / 12.92f : powf((s + 0.055f) / 1.055f, 2.4f);
}

// IEEE 754 binary16 (half-float) conversion for FP16 HDR texture upload.
static inline uint16_t float_to_half(float value)
{
    uint32_t f32;
    memcpy(&f32, &value, 4);
    uint32_t sign = (f32 >> 16) & 0x8000;
    int32_t  exp  = (int32_t)((f32 >> 23) & 0xFF) - 127;
    uint32_t mant = f32 & 0x7FFFFF;

    if (exp > 15)       return (uint16_t)(sign | 0x7C00);            // overflow → +/-inf
    if (exp > -15)      return (uint16_t)(sign | ((exp + 15) << 10) | (mant >> 13)); // normal
    if (exp > -25) {                                                  // denormal
        mant |= 0x800000;
        uint32_t shift = (uint32_t)(-14 - exp);
        return (uint16_t)(sign | (mant >> (shift + 13)));
    }
    return (uint16_t)sign;                                           // too small → zero
}

// ── HDR texture pipeline ─────────────────────────────────────────────
//
// The overlay implements the same pipeline used by Steam Overlay, Xbox
// Game Bar, and NVIDIA overlays for correct HDR compositing:
//
//   1. Sample the SDR (sRGB) source texture
//   2. Convert sRGB → linear  (sRGB EOTF)
//   3. Apply SDR white gain   (≈ 80–200 nits → display white level)
//   4. Write into an FP16 HDR render target
//
// For per-colour transforms (ImGui style colours), steps 1-3 happen
// per-channel in transform_color_for_swapchain() every frame.
//
// For textures (icons, avatars, images), we perform steps 1-4 at upload
// time by converting RGBA8 sRGB pixels to R16G16B16A16_FLOAT with the
// colour-space transform baked in.  This avoids the lossy 8-bit
// quantisation + Reinhard tonemap that the old RGBA8 path required.
// ─────────────────────────────────────────────────────────────────────

// PQ (ST.2084) constants
static constexpr float PQ_m1 = 0.1593017578125f;
static constexpr float PQ_m2 = 78.84375f;
static constexpr float PQ_c1 = 0.8359375f;
static constexpr float PQ_c2 = 18.8515625f;
static constexpr float PQ_c3 = 18.6875f;

// Apply per-image brightness / contrast / gamma adjustments in linear space.
// Order: gamma (power curve) → contrast (expand around 0.5) → brightness (scale).
static inline float apply_image_adjustments(float lin, float brightness, float contrast, float gamma_adj)
{
    if (gamma_adj != 1.0f && lin > 0.0f)
        lin = powf(lin, gamma_adj);
    if (contrast != 1.0f)
        lin = (lin - 0.5f) * contrast + 0.5f;
    if (brightness != 1.0f)
        lin *= brightness;
    return lin < 0.0f ? 0.0f : lin;
}

// Transform RGBA8 sRGB pixel buffer to FP16 (R16G16B16A16_FLOAT) for the
// current swapchain colour space.  Returns a uint16_t array with 4 halfs
// per pixel (RGBA order).  Caller owns the returned vector.
static std::vector<uint16_t> transform_pixels_to_fp16(const uint8_t *pixels, int w, int h,
                                                      SwapchainColorSpace cs, float sdr_scale,
                                                      float brightness = 1.0f, float contrast = 1.0f,
                                                      float gamma_adj = 1.0f)
{
    const int pixel_count = w * h;
    std::vector<uint16_t> out(pixel_count * 4);
    const bool has_adj = (brightness != 1.0f || contrast != 1.0f || gamma_adj != 1.0f);

    if (cs == SCS_LINEAR_HDR) {
        // sRGB → linear × SDR white scale.  No tonemap needed — FP16 stores > 1.0.
        for (int i = 0; i < pixel_count; ++i) {
            const uint8_t *src = pixels + i * 4;
            uint16_t      *dst = out.data() + i * 4;
            for (int c = 0; c < 3; ++c) {
                float lin = srgb_to_linear(src[c] / 255.0f);
                if (has_adj) lin = apply_image_adjustments(lin, brightness, contrast, gamma_adj);
                dst[c] = float_to_half(lin * sdr_scale);
            }
            dst[3] = float_to_half(src[3] / 255.0f);  // alpha: linear pass-through
        }
    } else if (cs == SCS_HDR10_PQ) {
        // sRGB → linear → PQ (ST.2084) encode
        const float nits = sdr_scale * 80.0f;
        for (int i = 0; i < pixel_count; ++i) {
            const uint8_t *src = pixels + i * 4;
            uint16_t      *dst = out.data() + i * 4;
            for (int c = 0; c < 3; ++c) {
                float lin = srgb_to_linear(src[c] / 255.0f);
                if (has_adj) lin = apply_image_adjustments(lin, brightness, contrast, gamma_adj);
                float L = (lin * nits) / 10000.0f;
                if (L < 0.0f) L = 0.0f;
                float Lm1 = powf(L, PQ_m1);
                dst[c] = float_to_half(powf((PQ_c1 + PQ_c2 * Lm1) / (1.0f + PQ_c3 * Lm1), PQ_m2));
            }
            dst[3] = float_to_half(src[3] / 255.0f);
        }
    } else if (cs == SCS_SDR_SRGB_RTV) {
        // sRGB → linear only (hw re-encodes sRGB on write to _SRGB RTV)
        for (int i = 0; i < pixel_count; ++i) {
            const uint8_t *src = pixels + i * 4;
            uint16_t      *dst = out.data() + i * 4;
            for (int c = 0; c < 3; ++c) {
                float lin = srgb_to_linear(src[c] / 255.0f);
                if (has_adj) lin = apply_image_adjustments(lin, brightness, contrast, gamma_adj);
                dst[c] = float_to_half(lin);
            }
            dst[3] = float_to_half(src[3] / 255.0f);
        }
    } else {
        // SDR / Unknown: 8-bit → FP16 with optional adjustments
        for (int i = 0; i < pixel_count; ++i) {
            const uint8_t *src = pixels + i * 4;
            uint16_t      *dst = out.data() + i * 4;
            for (int c = 0; c < 3; ++c) {
                float v = src[c] / 255.0f;
                if (has_adj) {
                    float lin = srgb_to_linear(v);
                    lin = apply_image_adjustments(lin, brightness, contrast, gamma_adj);
                    // Re-encode to sRGB since SDR expects sRGB values
                    v = lin <= 0.0031308f ? lin * 12.92f : 1.055f * powf(lin, 1.0f / 2.4f) - 0.055f;
                    if (v < 0.0f) v = 0.0f; else if (v > 1.0f) v = 1.0f;
                }
                dst[c] = float_to_half(v);
            }
            dst[3] = float_to_half(src[3] / 255.0f);
        }
    }
    return out;
}

// Track the colour space + SDR scale + image adjustments that cached textures
// were uploaded with.  When any of these change, the cache must be invalidated.
static SwapchainColorSpace s_cached_tex_cs    = SCS_UNKNOWN;
static float               s_cached_tex_scale = 1.0f;
static float               s_cached_tex_brightness = 1.0f;
static float               s_cached_tex_contrast   = 1.0f;
static float               s_cached_tex_gamma_adj  = 1.0f;

// Transform a single sRGB ImVec4 colour for the current swapchain colour space.
// Alpha is preserved untouched.
static ImVec4 transform_color_for_swapchain(const ImVec4 &col, SwapchainColorSpace cs, float sdr_scale)
{
    if (cs == SCS_SDR_UNORM || cs == SCS_UNKNOWN) return col;

    ImVec4 out = col;
    if (cs == SCS_LINEAR_HDR) {
        out.x = srgb_to_linear(col.x) * sdr_scale;
        out.y = srgb_to_linear(col.y) * sdr_scale;
        out.z = srgb_to_linear(col.z) * sdr_scale;
    } else if (cs == SCS_HDR10_PQ) {
        float nits = sdr_scale * 80.0f;
        auto ch_to_pq = [nits](float s) {
            float lin = srgb_to_linear(s);
            float L = (lin * nits) / 10000.0f;
            if (L < 0.0f) L = 0.0f;
            float Lm1 = powf(L, PQ_m1);
            return powf((PQ_c1 + PQ_c2 * Lm1) / (1.0f + PQ_c3 * Lm1), PQ_m2);
        };
        out.x = ch_to_pq(col.x);
        out.y = ch_to_pq(col.y);
        out.z = ch_to_pq(col.z);
    } else if (cs == SCS_SDR_SRGB_RTV) {
        out.x = srgb_to_linear(col.x);
        out.y = srgb_to_linear(col.y);
        out.z = srgb_to_linear(col.z);
    }
    return out;
}

// Shorthand macros for inline colours.  HDR transforms are applied only to
// textures (via transform_pixels_to_fp16); ReShade handles the ImGui colour-
// space conversion internally, so UI colours are passed through unchanged.
#define TC(c) (c)
#define TC32(c) (c)

// Transform RGBA8 pixel buffer in-place for the current swapchain colour space.
// Same logic as native overlay's srgb_decode_pixels_if_needed().
static void transform_pixels_for_swapchain(uint8_t *pixels, int w, int h,
                                           SwapchainColorSpace cs, float sdr_scale)
{
    if (!pixels || cs == SCS_SDR_UNORM || cs == SCS_UNKNOWN) return;

    const int pixel_count = w * h;
    const int byte_count  = pixel_count * 4;

    if (cs == SCS_LINEAR_HDR) {
        // sRGB → linear + SDR white scale, with Reinhard soft-shoulder if scale > 1
        const bool need_tonemap = sdr_scale > 1.01f;
        for (int i = 0; i < byte_count; i += 4) {
            for (int c = 0; c < 3; ++c) {
                float v = pixels[i + c] / 255.0f;
                float lin = srgb_to_linear(v) * sdr_scale;
                if (need_tonemap) lin = lin / (1.0f + lin); // Reinhard
                int b = (int)(lin * 255.0f + 0.5f);
                pixels[i + c] = (uint8_t)(b < 0 ? 0 : (b > 255 ? 255 : b));
            }
        }
    } else if (cs == SCS_HDR10_PQ) {
        // sRGB → linear → PQ (ST.2084) encode
        const float nits = sdr_scale * 80.0f;
        for (int i = 0; i < byte_count; i += 4) {
            for (int c = 0; c < 3; ++c) {
                float v = pixels[i + c] / 255.0f;
                float lin = srgb_to_linear(v);
                float L = (lin * nits) / 10000.0f;
                if (L < 0.0f) L = 0.0f;
                float Lm1 = powf(L, PQ_m1);
                float pq = powf((PQ_c1 + PQ_c2 * Lm1) / (1.0f + PQ_c3 * Lm1), PQ_m2);
                int b = (int)(pq * 255.0f + 0.5f);
                pixels[i + c] = (uint8_t)(b < 0 ? 0 : (b > 255 ? 255 : b));
            }
        }
    } else if (cs == SCS_SDR_SRGB_RTV) {
        // sRGB → linear only (hw re-encodes sRGB on write to _SRGB RTV)
        for (int i = 0; i < byte_count; i += 4) {
            for (int c = 0; c < 3; ++c) {
                float v = pixels[i + c] / 255.0f;
                float lin = srgb_to_linear(v);
                int b = (int)(lin * 255.0f + 0.5f);
                pixels[i + c] = (uint8_t)(b < 0 ? 0 : (b > 255 ? 255 : b));
            }
        }
    }
}

/* ── Helper: apply native overlay style colors ────────────────────────── */

static int apply_global_style_colors()
{
    int count = 0;
    auto push = [&](ImGuiCol idx, const ImVec4 &col) {
        ImGui::PushStyleColor(idx, col);
        ++count;
    };
    push(ImGuiCol_WindowBg, COL_MAIN_BG);
    push(ImGuiCol_TitleBgActive, COL_ELEMENT);
    push(ImGuiCol_Button, COL_ELEMENT);
    push(ImGuiCol_ButtonHovered, COL_ELEMENT_HOVER);
    push(ImGuiCol_FrameBg, COL_ELEMENT);
    push(ImGuiCol_FrameBgHovered, COL_ELEMENT_HOVER);
    push(ImGuiCol_ResizeGrip, COL_ELEMENT);
    push(ImGuiCol_ResizeGripHovered, COL_ELEMENT_HOVER);
    push(ImGuiCol_HeaderHovered, COL_ELEMENT_HOVER);
    return count;
}

/* ── Try to find and connect to the emu DLL ───────────────────────────── */

static bool try_connect_bridge()
{
    if (s_bridge_ok) return true;

    // Try common emu DLL names (includes steamclient for coldloader/steamclient_loader mode)
    const char *names[] = { "steam_api64.dll", "steam_api.dll", "steamclient64.dll", "steamclient.dll" };
    for (auto name : names) {
        s_emu_dll = GetModuleHandleA(name);
        if (s_emu_dll) break;
    }

    if (!s_emu_dll) return false;

    if (!GSE_LoadBridgeFunctions(s_emu_dll, &s_bridge)) return false;

    uint32_t ver = s_bridge.GetVersion();
    if (ver < GSE_BRIDGE_ABI_VERSION) {
        char msg[128];
        snprintf(msg, sizeof(msg), "GSE bridge ABI version mismatch (emu=%u, addon=%u)", ver, GSE_BRIDGE_ABI_VERSION);
        reshade::log::message(reshade::log::level::warning, msg);
    }

    s_bridge_ok = true;
    {
        char msg[64];
        snprintf(msg, sizeof(msg), "GSE bridge connected (ABI v%u)", ver);
        reshade::log::message(reshade::log::level::info, msg);
    }

    // Get the current language for translations
    if (s_bridge.GetLanguage) {
        s_current_language = s_bridge.GetLanguage();
        if (s_current_language < 0 || s_current_language >= TRANSLATION_NUMBER_OF_LANGUAGES)
            s_current_language = 0;
    }

    // Cache the SCE storage path
    if (s_bridge.GetSceStoragePath) {
        char path_buf[1024]{};
        s_bridge.GetSceStoragePath(path_buf, sizeof(path_buf));
        s_sce_storage_path = path_buf;
    }

    return true;
}

/* ── Helper: upload RGBA pixels to a GPU texture ──────────────────────── */

static IconTexture upload_icon(device *dev, const uint8_t *pixels, int w, int h)
{
    IconTexture icon{};
    if (!dev || !pixels || w <= 0 || h <= 0) return icon;

    // ── HDR texture pipeline ─────────────────────────────────────────
    // When the swap chain is HDR or _SRGB, upload textures as FP16
    // (R16G16B16A16_FLOAT) with the sRGB → linear → HDR transform baked
    // in.  This avoids the lossy 8-bit quantisation and Reinhard tonemap
    // that the legacy RGBA8 path required.
    //
    // For SDR swap chains, the classic RGBA8 path is used unchanged.
    // ──────────────────────────────────────────────────────────────────
    const bool use_fp16 = (s_addon_ecs == SCS_LINEAR_HDR ||
                           s_addon_ecs == SCS_HDR10_PQ   ||
                           s_addon_ecs == SCS_SDR_SRGB_RTV);
    const float img_brightness = s_appearance.image_brightness;
    const float img_contrast   = s_appearance.image_contrast;
    const float img_gamma_adj  = s_appearance.image_gamma_adjust;
    const bool has_adj = (img_brightness != 1.0f || img_contrast != 1.0f || img_gamma_adj != 1.0f);

    subresource_data init{};
    format tex_fmt;
    std::vector<uint16_t> fp16_pixels;

    if (use_fp16 || has_adj) {
        // FP16 path: sRGB → linear → adjustments → HDR gain → R16G16B16A16_FLOAT
        // Also used when image adjustments are active (even on SDR) for better precision.
        fp16_pixels = transform_pixels_to_fp16(pixels, w, h, s_addon_ecs, s_addon_sdr_scale,
                                                img_brightness, img_contrast, img_gamma_adj);
        init.data       = fp16_pixels.data();
        init.row_pitch  = w * 4 * sizeof(uint16_t);  // 4 × FP16 per pixel
        init.slice_pitch = w * h * 4 * (uint32_t)sizeof(uint16_t);
        tex_fmt = format::r16g16b16a16_float;
    } else {
        // SDR path: plain RGBA8 passthrough (no transform needed)
        init.data       = const_cast<uint8_t*>(pixels);
        init.row_pitch  = w * 4;
        init.slice_pitch = w * h * 4;
        tex_fmt = format::r8g8b8a8_unorm;
    }

    resource_desc desc(static_cast<uint32_t>(w), static_cast<uint32_t>(h),
        1, 1, tex_fmt, 1, memory_heap::default_,
        resource_usage::shader_resource);

    if (!dev->create_resource(desc, &init, resource_usage::shader_resource, &icon.tex)) {
        return icon;
    }

    if (!dev->create_resource_view(icon.tex, resource_usage::shader_resource,
            resource_view_desc(tex_fmt), &icon.srv)) {
        dev->destroy_resource(icon.tex);
        icon.tex = {};
        return icon;
    }

    icon.w = w;
    icon.h = h;
    icon.valid = true;
    return icon;
}

static void free_icon(device *dev, IconTexture &icon)
{
    if (!dev) return;
    if (icon.srv.handle) dev->destroy_resource_view(icon.srv);
    if (icon.tex.handle) dev->destroy_resource(icon.tex);
    icon = {};
}

/* ── Device lifecycle ─────────────────────────────────────────────────── */

static void on_init_device(device *dev)
{
    dev->create_private_data<addon_device_data>();
}

static void on_destroy_device(device *dev)
{
    if (s_current_device == dev)
        s_current_device = nullptr;
    auto *data = dev->get_private_data<addon_device_data>();
    if (data) {
        for (auto &[key, icon] : data->icon_cache)
            free_icon(dev, icon);
        dev->destroy_private_data<addon_device_data>();
    }
}

// Called after the swapchain is created / reset — cache the pointer so we can
// query get_color_space() for HDR detection (effect_runtime does NOT expose it).
static void on_init_swapchain(swapchain *sc, bool)
{
    s_current_swapchain = sc;
}

// Called before D3D9 Reset / DXGI ResizeBuffers / vkDestroySwapchain etc.
// D3D9 D3DPOOL_DEFAULT resources become invalid on Reset — must release them.
// D3D11/D3D12/Vulkan/OpenGL textures are device-level and survive resize, so skip.
static void on_destroy_swapchain(swapchain *sc, bool)
{
    if (s_current_swapchain == sc)
        s_current_swapchain = nullptr;

    device *dev = sc->get_device();
    if (!dev) return;
    if (dev->get_api() != device_api::d3d9) return;

    auto *data = dev->get_private_data<addon_device_data>();
    if (!data) return;

    for (auto &[key, icon] : data->icon_cache)
        free_icon(dev, icon);
    data->icon_cache.clear();
    for (auto &[key, icon] : data->avatar_cache)
        free_icon(dev, icon);
    data->avatar_cache.clear();
    for (auto &icon : data->pending_destroy)
        free_icon(dev, icon);
    data->pending_destroy.clear();
}

/* ── Helper: get or upload a cached icon by key ───────────────────────── */

static const IconTexture *get_or_upload_icon(const char *key, const uint8_t *pixels, int w, int h)
{
    if (!s_current_device || !pixels || w <= 0 || h <= 0 || !key || !key[0])
        return nullptr;

    auto *data = s_current_device->get_private_data<addon_device_data>();
    if (!data) return nullptr;

    auto it = data->icon_cache.find(key);
    if (it != data->icon_cache.end() && it->second.valid)
        return &it->second;

    IconTexture icon = upload_icon(s_current_device, pixels, w, h);
    if (!icon.valid) return nullptr;

    auto &stored = data->icon_cache[key];
    stored = icon;
    return &stored;
}

/* ── Helper: load an image from disk (stb_image) and upload to GPU ────── */

static const IconTexture *get_or_load_image(const std::string &file_path)
{
    if (!s_current_device || file_path.empty()) return nullptr;

    auto *data = s_current_device->get_private_data<addon_device_data>();
    if (!data) return nullptr;

    auto it = data->icon_cache.find(file_path);
    if (it != data->icon_cache.end()) {
        return it->second.valid ? &it->second : nullptr;
    }

    // Respect per-frame load cap
    if (s_sce_tex_per_frame >= MAX_SCE_TEX_PER_FRAME) return nullptr;
    ++s_sce_tex_per_frame;

    // Insert a placeholder (even on failure) so we don't retry every frame
    IconTexture placeholder{};
    data->icon_cache[file_path] = placeholder;

    int w = 0, h = 0;
    uint8_t *pixels = stbi_load(file_path.c_str(), &w, &h, nullptr, 4);
    if (!pixels) return nullptr;

    IconTexture icon = upload_icon(s_current_device, pixels, w, h);
    stbi_image_free(pixels);

    if (!icon.valid) return nullptr;

    data->icon_cache[file_path] = icon;
    return &data->icon_cache[file_path];
}

/* ── Helper: load a preview image immediately (no frame cap) ──────────── */

static const IconTexture *load_preview_image(const std::string &file_path)
{
    if (!s_current_device || file_path.empty()) return nullptr;

    auto *data = s_current_device->get_private_data<addon_device_data>();
    if (!data) return nullptr;

    auto it = data->icon_cache.find(file_path);
    if (it != data->icon_cache.end())
        return it->second.valid ? &it->second : nullptr;

    int w = 0, h = 0;
    uint8_t *pixels = stbi_load(file_path.c_str(), &w, &h, nullptr, 4);
    if (!pixels) {
        data->icon_cache[file_path] = IconTexture{};
        return nullptr;
    }

    IconTexture icon = upload_icon(s_current_device, pixels, w, h);
    stbi_image_free(pixels);

    if (!icon.valid) {
        data->icon_cache[file_path] = IconTexture{};
        return nullptr;
    }

    data->icon_cache[file_path] = icon;
    return &data->icon_cache[file_path];
}

/* ── Helper: get or create avatar texture for a friend ────────────────── */

static const IconTexture *get_or_upload_avatar(uint64_t steam_id)
{
    if (!s_current_device || !s_bridge.GetAvatar) return nullptr;
    
    auto *data = s_current_device->get_private_data<addon_device_data>();
    if (!data) return nullptr;
    
    // Check cache first
    auto it = data->avatar_cache.find(steam_id);
    if (it != data->avatar_cache.end()) {
        return it->second.valid ? &it->second : nullptr;
    }
    
    // Fetch avatar from bridge
    GSE_AvatarData avatar{};
    if (!s_bridge.GetAvatar(steam_id, &avatar) || !avatar.valid) {
        // Insert invalid placeholder to avoid re-fetching
        data->avatar_cache[steam_id] = IconTexture{};
        return nullptr;
    }
    
    // Upload to GPU
    IconTexture tex = upload_icon(s_current_device, avatar.pixels, GSE_AVATAR_SIZE, GSE_AVATAR_SIZE);
    data->avatar_cache[steam_id] = tex;
    
    return tex.valid ? &data->avatar_cache[steam_id] : nullptr;
}

static const IconTexture *get_or_upload_local_avatar()
{
    if (!s_current_device || !s_bridge.GetLocalAvatar) return nullptr;
    
    auto *data = s_current_device->get_private_data<addon_device_data>();
    if (!data) return nullptr;
    
    // Use 0 as key for local avatar
    auto it = data->avatar_cache.find(0);
    if (it != data->avatar_cache.end()) {
        return it->second.valid ? &it->second : nullptr;
    }
    
    // Fetch local avatar from bridge
    GSE_AvatarData avatar{};
    if (!s_bridge.GetLocalAvatar(&avatar) || !avatar.valid) {
        data->avatar_cache[0] = IconTexture{};
        return nullptr;
    }
    
    IconTexture tex = upload_icon(s_current_device, avatar.pixels, GSE_AVATAR_SIZE, GSE_AVATAR_SIZE);
    data->avatar_cache[0] = tex;
    
    return tex.valid ? &data->avatar_cache[0] : nullptr;
}

/* ── Helper: free all SCE textures from the cache ─────────────────────── */

static void free_sce_textures()
{
    if (!s_current_device) return;
    auto *data = s_current_device->get_private_data<addon_device_data>();
    if (!data) return;

    // Move SCE entries to pending_destroy so they are freed at the start of the next frame,
    // after the GPU has finished using them.  Destroying mid-frame causes DXGI device hung.
    for (auto it = data->icon_cache.begin(); it != data->icon_cache.end(); ) {
        if (it->first.size() > 4 && (it->first.find("sce_assets") != std::string::npos ||
            it->first.find("Series ") != std::string::npos)) {
            if (it->second.valid)
                data->pending_destroy.push_back(it->second);
            it = data->icon_cache.erase(it);
        } else {
            ++it;
        }
    }
}

/* ── SCE browser helper: sanitize filename (same as native) ───────────── */

static std::string sce_sanitize(const char *name)
{
    std::string s(name);
    for (char &c : s) {
        if (c == '/' || c == '\\' || c == ':' || c == '*' ||
            c == '?' || c == '"'  || c == '<' || c == '>' || c == '|')
            c = '_';
    }
    if (s.size() > 48) s.resize(48);
    return s;
}

/* ── SCE browser helper: extract file extension from URL ──────────────── */

static std::string sce_url_ext(const char *url)
{
    if (!url || !url[0]) return ".png";
    std::string u(url);
    size_t slash = u.rfind('/');
    std::string name = (slash != std::string::npos) ? u.substr(slash + 1) : u;
    size_t q = name.find('?');
    if (q != std::string::npos) name.resize(q);
    size_t dot = name.rfind('.');
    return (dot != std::string::npos) ? name.substr(dot) : ".png";
}

/* ── SCE browser helper: type subfolder name (same as native) ─────────── */

static const char *sce_type_subfolder(int tidx)
{
    static constexpr const char *dirs[] = {
        "cards", "foil_cards", "booster_packs",
        "badges", "foil_badges", "emoticons",
        "backgrounds", "animated_backgrounds", "animated_mini_backgrounds",
        "profiles", "avatar_frames", "animated_avatars",
        "animated_stickers", "startup_movies"
    };
    return (tidx >= 0 && tidx < 14) ? dirs[tidx] : "misc";
}

/* ── SCE browser helper: rarity colour ────────────────────────────────── */

static ImVec4 sce_rarity_color(const char *rarity)
{
    if (!rarity) return ImVec4(0.60f, 0.60f, 0.60f, 1.0f);
    if (strcmp(rarity, "Uncommon") == 0) return ImVec4(0.20f, 0.85f, 0.20f, 1.0f);
    if (strcmp(rarity, "Rare") == 0)     return ImVec4(0.90f, 0.20f, 0.20f, 1.0f);
    return ImVec4(0.60f, 0.60f, 0.60f, 1.0f);
}

/* ── Notification rendering (always visible, via reshade_overlay event) ─ */

// Animation factor: 0.0 = fully visible, 1.0 = fully off-screen
static float animate_factor(int64_t elapsed_ms, int64_t display_duration_ms)
{
    // Phase 1: slide in
    if (elapsed_ms < ANIM_DURATION_MS) {
        return 1.0f - (float)elapsed_ms / (float)ANIM_DURATION_MS;
    }
    // Phase 2: steady
    if (elapsed_ms < ANIM_DURATION_MS + display_duration_ms) {
        return 0.0f;
    }
    // Phase 3: slide out
    int64_t out_elapsed = elapsed_ms - ANIM_DURATION_MS - display_duration_ms;
    if (out_elapsed < ANIM_DURATION_MS) {
        return (float)out_elapsed / (float)ANIM_DURATION_MS;
    }
    return 1.0f;
}

// Accumulated Y offsets per notification region (reset each frame)
struct NotifCoords {
    float top_left = 0, top_center = 0, top_right = 0;
    float bot_left = 0, bot_center = 0, bot_right = 0;
};

// Cache actual rendered notification size per ID for accurate stacking
static std::unordered_map<int, ImVec2> s_notif_size_cache;

static void render_notifications(effect_runtime *runtime)
{
    if (!s_bridge_ok || !s_bridge.GetNotifications) return;

    GSE_Notification notifs[16];
    int count = s_bridge.GetNotifications(notifs, 16);
    if (count <= 0) return;

    auto &io = ImGui::GetIO();
    float screen_w = io.DisplaySize.x;
    float screen_h = io.DisplaySize.y;

    float min_notif_w = screen_w * NOTIF_WIDTH_FRAC;
    if (min_notif_w < 300.0f) min_notif_w = 300.0f;

    auto now_ms = std::chrono::duration_cast<std::chrono::milliseconds>(
        std::chrono::steady_clock::now().time_since_epoch()).count();

    ImGui::PushStyleVar(ImGuiStyleVar_WindowRounding, NOTIF_ROUNDING);

    // Pre-fetch friends list for avatar + info rendering in friend-related notifications
    std::vector<GSE_Friend> notif_friends;
    if (s_bridge.GetFriendCount && s_bridge.GetFriends) {
        int fc = s_bridge.GetFriendCount();
        if (fc > 0) {
            notif_friends.resize(fc > 256 ? 256 : fc);
            int cnt = s_bridge.GetFriends(notif_friends.data(), (int)notif_friends.size());
            notif_friends.resize(cnt);
        }
    }

    // Helper: render 48px avatar + 3-line friend info for a notification
    auto render_notif_friend_header = [&](uint64_t friend_id) {
        if (!friend_id) return;

        const GSE_Friend *finfo = nullptr;
        for (auto &f : notif_friends) {
            if (f.steam_id == friend_id) { finfo = &f; break; }
        }
        if (!finfo) return;

        const float avatar_size = 48.0f;

        const IconTexture *avatar = get_or_upload_avatar(friend_id);
        if (avatar && avatar->valid) {
            ImGui::Image(ImTextureRef(avatar->srv.handle), ImVec2(avatar_size, avatar_size));
        } else {
            ImVec2 p = ImGui::GetCursorScreenPos();
            ImGui::GetWindowDrawList()->AddRectFilled(p, ImVec2(p.x + avatar_size, p.y + avatar_size), TC32(IM_COL32(60, 60, 80, 255)));
            ImGui::Dummy(ImVec2(avatar_size, avatar_size));
        }
        ImGui::SameLine();

        ImVec2 text_start = ImGui::GetCursorPos();

        // Line 1: Name (ID: steamid)
        ImGui::SetCursorPos(text_start);
        ImGui::TextColored(TC(ImVec4(0.9f, 0.9f, 0.2f, 1.0f)), "%s", finfo->name);
        ImGui::SameLine();
        ImGui::TextColored(TC(ImVec4(0.6f, 0.6f, 0.6f, 1.0f)), "(ID: %llu)", (unsigned long long)finfo->steam_id);

        // Line 2: Playing AppName (AppID XXXX)
        ImGui::SetCursorPosX(text_start.x);
        if (finfo->appid != 0) {
            if (finfo->app_name[0])
                ImGui::TextColored(TC(ImVec4(0.5f, 0.8f, 0.5f, 1.0f)), "Playing %s (AppID %u)", finfo->app_name, finfo->appid);
            else
                ImGui::TextColored(TC(ImVec4(0.5f, 0.8f, 0.5f, 1.0f)), "Playing AppID %u", finfo->appid);
        } else {
            ImGui::TextColored(TC(ImVec4(0.5f, 0.5f, 0.5f, 1.0f)), "Online");
        }

        // Line 3: Lobby info
        ImGui::SetCursorPosX(text_start.x);
        if (finfo->in_lobby && finfo->lobby_id != 0) {
            bool frd_is_owner = (finfo->lobby_owner_name[0] && strcmp(finfo->lobby_owner_name, finfo->name) == 0);
            if (finfo->lobby_owner_name[0])
                ImGui::TextColored(TC(ImVec4(0.4f, 0.8f, 0.4f, 1.0f)), "%s - %llu (%d/%d - %s)",
                    frd_is_owner ? "Has Lobby" : "In Lobby",
                    (unsigned long long)finfo->lobby_id, finfo->lobby_member_count, finfo->lobby_member_limit, finfo->lobby_owner_name);
            else
                ImGui::TextColored(TC(ImVec4(0.4f, 0.8f, 0.4f, 1.0f)), "In Lobby - %llu (%d/%d)",
                    (unsigned long long)finfo->lobby_id, finfo->lobby_member_count, finfo->lobby_member_limit);
        }

        ImGui::Separator();
    };

    NotifCoords coords{};

    for (int i = 0; i < count; ++i) {
        auto &n = notifs[i];

        int64_t elapsed = now_ms - n.start_time_ms;
        if (elapsed < 0) elapsed = 0;

        // Total lifetime = anim_in + display + anim_out
        int64_t total = n.duration_ms + ANIM_DURATION_MS * 2;
        if (n.duration_ms > 0 && elapsed > total) {
            if (s_bridge.ExpireNotification)
                s_bridge.ExpireNotification(n.id);
            continue;
        }

        int64_t display_dur = n.duration_ms > 0 ? n.duration_ms : 7000;
        float factor = animate_factor(elapsed, display_dur);
        float alpha = 1.0f - factor;

        // Compute notification height estimate (used for stacking, not window size)
        float font_size = ImGui::GetFontSize();
        float padding = ImGui::GetStyle().WindowPadding.y * 2.0f;
        float notif_h = 0.0f;
        bool is_achievement = (n.type == GSE_NOTIF_ACHIEVEMENT || n.type == GSE_NOTIF_ACHIEVEMENT_PROG);
        bool has_progress = (n.type == GSE_NOTIF_ACHIEVEMENT_PROG ||
                            (n.type == GSE_NOTIF_ACHIEVEMENT && !n.ach_achieved)) &&
                            n.ach_max_progress > 0;

        if (is_achievement) {
            // Achievement: title + icon row + optional progress bar
            float title_h = font_size + ImGui::GetStyle().ItemSpacing.y;
            float row_h = ICON_SIZE;
            notif_h = title_h + row_h + padding;
            if (has_progress) notif_h += font_size + ImGui::GetStyle().WindowPadding.y;
        } else {
            // Non-achievement: friend header + message text + optional buttons
            float msg_h = font_size + ImGui::GetStyle().ItemSpacing.y;  // approximate 1 line of message text
            notif_h = msg_h + padding;

            // Friend avatar + 3-line header
            if (n.source_friend_id != 0) {
                float friend_hdr = (std::max)(48.0f, font_size * 3.0f) + ImGui::GetStyle().ItemSpacing.y + 1.0f;
                notif_h += friend_hdr;
            }

            // Button row for interactive notification types
            float btn_h = ImGui::GetFrameHeight() + ImGui::GetStyle().ItemSpacing.y;
            switch (n.type) {
            case GSE_NOTIF_INVITE:
            case GSE_NOTIF_LOBBY_JOIN_REQ:
            case GSE_NOTIF_LOBBY_JOIN_RESP:  // dismiss button
            case GSE_NOTIF_LOBBY_KICKED:    // dismiss button
            case GSE_NOTIF_FRIEND_LOBBY:    // "Request to Join" + "Close"
                notif_h += btn_h;
                break;
            case GSE_NOTIF_MESSAGE:
                if (n.source_friend_id) notif_h += btn_h; // "Open Chat" button
                break;
            default:
                break;
            }
        }

        // Use cached rendered size from previous frame if available
        float notif_w = min_notif_w;
        auto cache_it = s_notif_size_cache.find(n.id);
        if (!is_achievement && cache_it != s_notif_size_cache.end()) {
            if (cache_it->second.x > min_notif_w) notif_w = cache_it->second.x;
            notif_h = cache_it->second.y;
        }

        // Get position preference (per-type, matching native overlay)
        int notif_pos = GSE_NOTIF_POS_TOP_RIGHT; // fallback default
        if (s_bridge.GetOption) {
            switch (n.type) {
            case GSE_NOTIF_ACHIEVEMENT:
            case GSE_NOTIF_ACHIEVEMENT_PROG:
                notif_pos = s_bridge.GetOption(GSE_OPT_NOTIF_POS_ACHIEVEMENT);
                break;
            case GSE_NOTIF_INVITE:
            case GSE_NOTIF_LOBBY_JOIN_REQ:
            case GSE_NOTIF_LOBBY_JOIN_RESP:
            case GSE_NOTIF_LOBBY_KICKED:
            case GSE_NOTIF_LOBBY_STATUS:
                notif_pos = s_bridge.GetOption(GSE_OPT_NOTIF_POS_INVITE);
                break;
            case GSE_NOTIF_MESSAGE:
                notif_pos = s_bridge.GetOption(GSE_OPT_NOTIF_POS_CHAT);
                break;
            default:
                notif_pos = s_bridge.GetOption(GSE_OPT_NOTIF_POSITION);
                break;
            }
        }

        // Position calculation matching native overlay
        float slide = factor;
        float x = 0, y = 0;
        switch (notif_pos) {
        case GSE_NOTIF_POS_TOP_LEFT: {
            float anim_off = slide * notif_w;
            x = NOTIF_MARGIN_X - anim_off;
            y = coords.top_left + NOTIF_MARGIN_Y;
            coords.top_left = y + notif_h;
        } break;
        case GSE_NOTIF_POS_TOP_CENTER: {
            float anim_off = slide * notif_h;
            x = (screen_w - notif_w) * 0.5f;
            y = coords.top_center + NOTIF_MARGIN_Y - anim_off;
            coords.top_center = y + notif_h;
        } break;
        case GSE_NOTIF_POS_TOP_RIGHT: {
            float anim_off = slide * notif_w;
            x = screen_w - notif_w - NOTIF_MARGIN_X + anim_off;
            y = coords.top_right + NOTIF_MARGIN_Y;
            coords.top_right = y + notif_h;
        } break;
        case GSE_NOTIF_POS_BOT_LEFT: {
            float anim_off = slide * notif_w;
            x = NOTIF_MARGIN_X - anim_off;
            y = screen_h - coords.bot_left - NOTIF_MARGIN_Y - notif_h;
            coords.bot_left = screen_h - y;
        } break;
        case GSE_NOTIF_POS_BOT_CENTER: {
            float anim_off = slide * notif_h;
            x = (screen_w - notif_w) * 0.5f;
            y = screen_h - coords.bot_center - NOTIF_MARGIN_Y - notif_h + anim_off;
            coords.bot_center = screen_h - y;
        } break;
        default:
        case GSE_NOTIF_POS_BOT_RIGHT: {
            float anim_off = slide * notif_w;
            x = screen_w - notif_w - NOTIF_MARGIN_X + anim_off;
            y = screen_h - coords.bot_right - NOTIF_MARGIN_Y - notif_h;
            coords.bot_right = screen_h - y;
        } break;
        }

        ImGui::SetNextWindowPos(ImVec2(x, y));
        if (is_achievement) {
            ImGui::SetNextWindowSize(ImVec2(min_notif_w, 0));
        } else {
            ImGui::SetNextWindowSizeConstraints(ImVec2(min_notif_w, 0), ImVec2(screen_w * 0.5f, screen_h));
        }

        char win_id[64];
        snprintf(win_id, sizeof(win_id), "##gse_notif_%d", n.id);

        // Some notification types are non-interactive (like native overlay)
        ImGuiWindowFlags flags = ImGuiWindowFlags_NoMove | ImGuiWindowFlags_NoDecoration |
            ImGuiWindowFlags_NoResize | ImGuiWindowFlags_NoFocusOnAppearing |
            ImGuiWindowFlags_NoSavedSettings;

        switch (n.type) {
        case GSE_NOTIF_ACHIEVEMENT:
        case GSE_NOTIF_ACHIEVEMENT_PROG:
        case GSE_NOTIF_AUTO_ACCEPT_INVITE:
            flags |= ImGuiWindowFlags_NoBringToFrontOnFocus | ImGuiWindowFlags_NoInputs;
            break;
        case GSE_NOTIF_LOBBY_JOIN_RESP:
        case GSE_NOTIF_LOBBY_KICKED:
            // dismissable via button — keep NoBringToFrontOnFocus but allow input
            flags |= ImGuiWindowFlags_NoBringToFrontOnFocus;
            break;
        case GSE_NOTIF_MESSAGE:
            // Interactive if there's a source friend (Open Chat button), otherwise passive
            if (n.source_friend_id)
                flags |= ImGuiWindowFlags_NoBringToFrontOnFocus;
            else
                flags |= ImGuiWindowFlags_NoBringToFrontOnFocus | ImGuiWindowFlags_NoInputs;
            break;
        case GSE_NOTIF_INVITE:
        case GSE_NOTIF_LOBBY_JOIN_REQ:
        case GSE_NOTIF_FRIEND_LOBBY:
            // interactive: buttons remain clickable, but stay behind overlay windows
            if (s_show_main_overlay) flags |= ImGuiWindowFlags_NoBringToFrontOnFocus;
            break;
        case GSE_NOTIF_LOBBY_STATUS:
            // non-interactive but always visible on top when overlay is open
            flags |= ImGuiWindowFlags_NoInputs;
            break;
        default:
            break;
        }

        if (!is_achievement) flags |= ImGuiWindowFlags_AlwaysAutoResize;

        // Push native notification colors (transformed for swapchain colour space)
        ImGui::PushStyleColor(ImGuiCol_WindowBg, TC(COL_NOTIF_BG));
        ImGui::PushStyleColor(ImGuiCol_Border, TC(ImVec4(0, 0, 0, alpha)));
        ImGui::PushStyleColor(ImGuiCol_Text, TC(ImVec4(1.0f, 1.0f, 1.0f, alpha)));
        ImGui::PushStyleVar(ImGuiStyleVar_Alpha, alpha);

        if (ImGui::Begin(win_id, nullptr, flags)) {
            // Cache actual rendered size for accurate stacking next frame
            s_notif_size_cache[n.id] = ImGui::GetWindowSize();
            switch (n.type) {
            case GSE_NOTIF_ACHIEVEMENT:
            case GSE_NOTIF_ACHIEVEMENT_PROG: {
                bool is_unlock = (n.type == GSE_NOTIF_ACHIEVEMENT) && n.ach_achieved;

                // Title (centered)
                ImGui::Text("%s", n.ach_title[0] ? n.ach_title : "Achievement");

                // Icon + description table (matching native layout)
                bool has_icon = (n.ach_icon_pixels != nullptr && n.ach_icon_w > 0 && n.ach_icon_h > 0);
                if (has_icon && ImGui::BeginTable("##ach_notif_tbl", 2)) {
                    ImGui::TableSetupColumn("icon", ImGuiTableColumnFlags_WidthFixed, ICON_SIZE);
                    ImGui::TableSetupColumn("text");
                    ImGui::TableNextRow(ImGuiTableRowFlags_None, ICON_SIZE);

                    ImGui::TableSetColumnIndex(0);
                    {
                        // Try to render real icon texture
                        char icon_key[320];
                        snprintf(icon_key, sizeof(icon_key), "notif_%d", n.id);
                        const IconTexture *icon = get_or_upload_icon(icon_key, n.ach_icon_pixels, n.ach_icon_w, n.ach_icon_h);
                        if (icon) {
                            ImGui::Image(ImTextureRef(icon->srv.handle), ImVec2(ICON_SIZE, ICON_SIZE));
                        } else {
                            // Fallback: colored placeholder
                            ImVec4 icon_col = is_unlock
                                ? ImVec4(0.2f, 0.8f, 0.2f, alpha)
                                : ImVec4(0.5f, 0.5f, 0.5f, alpha);
                            ImGui::PushStyleColor(ImGuiCol_Button, icon_col);
                            ImGui::Button(is_unlock ? u8"\u2713" : "?", ImVec2(ICON_SIZE, ICON_SIZE));
                            ImGui::PopStyleColor();
                        }
                    }

                    ImGui::TableSetColumnIndex(1);
                    ImGui::TextWrapped("%s", n.message);

                    ImGui::EndTable();
                } else {
                    ImGui::TextWrapped("%s", n.message);
                }

                // Progress bar for in-progress achievements
                if (has_progress) {
                    float frac = (float)n.ach_progress / (float)n.ach_max_progress;
                    char prog_buf[32];
                    snprintf(prog_buf, sizeof(prog_buf), "%u/%u", n.ach_progress, n.ach_max_progress);
                    ImGui::ProgressBar(frac, ImVec2(-1, ImGui::GetFontSize()), prog_buf);
                }
                break;
            }
            case GSE_NOTIF_INVITE:
                render_notif_friend_header(n.source_friend_id);
                ImGui::TextWrapped("%s", n.message);
                if (ImGui::Button(translationJoin[s_current_language])) {
                    // Accept the invite via FriendAction (action=5)
                    if (s_bridge.FriendAction && n.source_friend_id)
                        s_bridge.FriendAction(n.source_friend_id, 5);
                    if (s_bridge.ExpireNotification)
                        s_bridge.ExpireNotification(n.id);
                }
                ImGui::SameLine();
                if (ImGui::Button(translationRefuse[s_current_language])) {
                    if (s_bridge.FriendAction && n.source_friend_id)
                        s_bridge.FriendAction(n.source_friend_id, 6); // GSE_FRIEND_ACTION_REFUSE_INVITE
                    if (s_bridge.ExpireNotification)
                        s_bridge.ExpireNotification(n.id);
                }
                break;
            case GSE_NOTIF_LOBBY_JOIN_REQ:
                render_notif_friend_header(n.source_friend_id);
                ImGui::TextWrapped("%s", n.message);
                if (ImGui::Button(translationJoin[s_current_language])) {
                    if (s_bridge.AcceptLobbyJoinRequest)
                        s_bridge.AcceptLobbyJoinRequest(n.id);
                }
                ImGui::SameLine();
                if (ImGui::Button(translationRefuse[s_current_language])) {
                    if (s_bridge.DeclineLobbyJoinRequest)
                        s_bridge.DeclineLobbyJoinRequest(n.id);
                }
                break;
            case GSE_NOTIF_LOBBY_JOIN_RESP:
                render_notif_friend_header(n.source_friend_id);
                ImGui::TextWrapped("%s", n.message);
                if (ImGui::Button(translationClose[s_current_language])) {
                    if (s_bridge.ExpireNotification)
                        s_bridge.ExpireNotification(n.id);
                }
                break;
            case GSE_NOTIF_LOBBY_KICKED:
                render_notif_friend_header(n.source_friend_id);
                ImGui::TextWrapped("%s", n.message);
                if (ImGui::Button(translationClose[s_current_language])) {
                    if (s_bridge.ExpireNotification)
                        s_bridge.ExpireNotification(n.id);
                }
                break;
            case GSE_NOTIF_FRIEND_LOBBY:
                render_notif_friend_header(n.source_friend_id);
                ImGui::TextWrapped("%s", n.message);
                if (ImGui::Button("Request to Join")) {
                    if (s_bridge.RequestJoinFriendLobby)
                        s_bridge.RequestJoinFriendLobby(n.id);
                }
                ImGui::SameLine();
                if (ImGui::Button(translationClose[s_current_language])) {
                    if (s_bridge.ExpireNotification)
                        s_bridge.ExpireNotification(n.id);
                }
                break;
            case GSE_NOTIF_AUTO_ACCEPT_INVITE:
                ImGui::TextWrapped("%s", n.message);
                break;
            case GSE_NOTIF_MESSAGE:
                render_notif_friend_header(n.source_friend_id);
                ImGui::TextWrapped("%s", n.message);
                if (n.source_friend_id && s_bridge.OpenChat) {
                    if (ImGui::Button("Open Chat")) {
                        s_bridge.OpenChat(n.source_friend_id);
                        if (s_bridge.ExpireNotification)
                            s_bridge.ExpireNotification(n.id);
                    }
                }
                break;
            default:
                ImGui::TextWrapped("%s", n.message);
                break;
            }
        }
        ImGui::End();

        ImGui::PopStyleVar();   // Alpha
        ImGui::PopStyleColor(3); // WindowBg, Border, Text
    }

    ImGui::PopStyleVar(); // WindowRounding

    // Clean up cached sizes for notifications that no longer exist
    for (auto it = s_notif_size_cache.begin(); it != s_notif_size_cache.end(); ) {
        bool found = false;
        for (int i = 0; i < count; ++i) {
            if (notifs[i].id == it->first) { found = true; break; }
        }
        if (!found) it = s_notif_size_cache.erase(it);
        else ++it;
    }
}

/* ── Stats HUD (always visible when enabled, matching native style) ────── */

static void render_stats_hud()
{
    if (!s_bridge_ok || !s_bridge.GetState) return;

    GSE_OverlayState state{};
    s_bridge.GetState(&state);

    bool any_stats = state.show_fps || state.show_frametime || state.show_playtime;
    if (!any_stats) return;

    int timeframe_sec = state.graph_timeframe_sec > 0 ? state.graph_timeframe_sec : 5;

    // Build the stats line
    char stats_text[256] = {};
    bool need_sep = false;

    if (state.show_fps) {
        char tmp[32];
        snprintf(tmp, sizeof(tmp), "FPS: %.1f", s_display_fps);
        strcat(stats_text, tmp);
        need_sep = true;
    }
    if (state.show_frametime) {
        if (need_sep) strcat(stats_text, " | ");
        char tmp[64];
        snprintf(tmp, sizeof(tmp), "Frametime: %.1fms", s_display_frametime);
        strcat(stats_text, tmp);
        need_sep = true;
    }
    if (state.show_playtime) {
        if (need_sep) strcat(stats_text, " | ");
        int hr, mn, sc;
        get_playtime(hr, mn, sc);
        char tmp[32];
        snprintf(tmp, sizeof(tmp), "Playtime: %02d:%02d:%02d", hr % 24, mn, sc);
        strcat(stats_text, tmp);
    }

    bool want_ft_graph = state.show_frametime && state.show_frametime_graph;
    bool want_fps_graph = state.show_fps && state.show_fps_graph;
    int vis_count = get_addon_visible_count(timeframe_sec);
    bool show_graph = (want_ft_graph || want_fps_graph) && vis_count > 1;
    bool need_sorted = state.show_percentile_1 || state.show_percentile_5 || state.show_percentile_01;
    float graph_height = 40.0f;

    // Build data arrays from visible window
    std::vector<float> ft_data, fps_data, sorted_ft;
    if (show_graph || state.show_min_max_avg || need_sorted) {
        ft_data.resize(vis_count);
        fps_data.resize(vis_count);
        sorted_ft.resize(vis_count);
        int ring_start = (s_ft_history_idx - vis_count + ADDON_FT_HISTORY_SIZE) % ADDON_FT_HISTORY_SIZE;
        for (int i = 0; i < vis_count; i++) {
            float ft = s_ft_history[(ring_start + i) % ADDON_FT_HISTORY_SIZE];
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

    float min_content_width = show_graph ? 260.0f : 0.0f;

    ImGuiWindowFlags flags = ImGuiWindowFlags_NoMove | ImGuiWindowFlags_NoDecoration |
        ImGuiWindowFlags_AlwaysAutoResize | ImGuiWindowFlags_NoFocusOnAppearing |
        ImGuiWindowFlags_NoBringToFrontOnFocus | ImGuiWindowFlags_NoInputs |
        ImGuiWindowFlags_NoCollapse | ImGuiWindowFlags_NoSavedSettings |
        ImGuiWindowFlags_NoScrollWithMouse;

    ImGui::PushStyleVar(ImGuiStyleVar_WindowRounding, NOTIF_ROUNDING);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowBorderSize, 0.0f);
    ImGui::PushStyleColor(ImGuiCol_WindowBg, TC(COL_STATS_BG));
    ImGui::PushStyleColor(ImGuiCol_Text, TC(COL_STATS_TEXT));

    if (ImGui::Begin("##gse_stats", nullptr, flags)) {
        // Anchor position based on actual window size (top-left default)
        ImVec2 win_sz = ImGui::GetWindowSize();
        float pos_x = 0.0f, pos_y = 0.0f;
        ImGui::SetWindowPos(ImVec2(
            ImGui::GetIO().DisplaySize.x * pos_x - win_sz.x * pos_x,
            ImGui::GetIO().DisplaySize.y * pos_y - win_sz.y * pos_y
        ));

        float content_width = ImGui::GetContentRegionAvail().x;
        if (content_width < min_content_width) content_width = min_content_width;

        ImGui::TextUnformatted(stats_text);

        // ---- Frametime graph + stats ----
        if (state.show_frametime && vis_count > 1) {
            if (state.show_frametime_graph) {
                ImGui::Spacing();
                ImGui::TextColored(TC(ImVec4(0.8f, 0.8f, 0.4f, 1.0f)), "Frametime");

                float ft_scale_max = s_ft_max * 1.2f;
                if (ft_scale_max < 1.0f) ft_scale_max = 1.0f;

                ImGui::PushStyleColor(ImGuiCol_PlotLines, TC(ImVec4(0.4f, 0.8f, 0.4f, 1.0f)));
                ImGui::PushStyleColor(ImGuiCol_FrameBg, TC(ImVec4(0.0f, 0.0f, 0.0f, 0.3f)));
                ImGui::PlotLines("##ft_graph", ft_data.data(), vis_count, 0, nullptr,
                    0.0f, ft_scale_max, ImVec2(content_width, graph_height));
                ImGui::PopStyleColor(2);
            }

            if (state.show_min_max_avg) {
                ImGui::TextColored(TC(ImVec4(0.7f, 0.7f, 0.7f, 1.0f)),
                    "Min: %.1fms  Avg: %.1fms  Max: %.1fms",
                    s_display_min_ft, s_display_avg_ft, s_display_max_ft);
            }

            if (need_sorted) {
                std::string pct_line;
                if (state.show_percentile_01) {
                    char buf[32]; snprintf(buf, sizeof(buf), "0.1%% high: %.1fms", ft_percentile(0.999f));
                    pct_line += buf;
                }
                if (state.show_percentile_1) {
                    if (!pct_line.empty()) pct_line += "  ";
                    char buf[32]; snprintf(buf, sizeof(buf), "1%% high: %.1fms", ft_percentile(0.99f));
                    pct_line += buf;
                }
                if (state.show_percentile_5) {
                    if (!pct_line.empty()) pct_line += "  ";
                    char buf[32]; snprintf(buf, sizeof(buf), "5%% high: %.1fms", ft_percentile(0.95f));
                    pct_line += buf;
                }
                ImGui::TextColored(TC(ImVec4(0.7f, 0.7f, 0.7f, 1.0f)), "%s", pct_line.c_str());
            }
        }

        // ---- FPS graph + stats ----
        if (state.show_fps && vis_count > 1) {
            float fps_min = (s_display_max_ft > 0.0f) ? (1000.0f / s_display_max_ft) : 0.0f;
            float fps_max = (s_display_min_ft > 0.0f) ? (1000.0f / s_display_min_ft) : 0.0f;
            float fps_avg = (s_display_avg_ft > 0.0f) ? (1000.0f / s_display_avg_ft) : 0.0f;

            if (state.show_fps_graph) {
                ImGui::Spacing();
                ImGui::TextColored(TC(ImVec4(0.8f, 0.8f, 0.4f, 1.0f)), "FPS");

                float fps_scale_max = fps_max * 1.2f;
                if (fps_scale_max < 1.0f) fps_scale_max = 1.0f;

                ImGui::PushStyleColor(ImGuiCol_PlotLines, TC(ImVec4(0.4f, 0.6f, 1.0f, 1.0f)));
                ImGui::PushStyleColor(ImGuiCol_FrameBg, TC(ImVec4(0.0f, 0.0f, 0.0f, 0.3f)));
                ImGui::PlotLines("##fps_graph", fps_data.data(), vis_count, 0, nullptr,
                    0.0f, fps_scale_max, ImVec2(content_width, graph_height));
                ImGui::PopStyleColor(2);
            }

            if (state.show_min_max_avg) {
                ImGui::TextColored(TC(ImVec4(0.7f, 0.7f, 0.7f, 1.0f)),
                    "Min: %.0f  Avg: %.0f  Max: %.0f",
                    fps_min, fps_avg, fps_max);
            }

            if (need_sorted) {
                std::string pct_line;
                if (state.show_percentile_01) {
                    char buf[32]; snprintf(buf, sizeof(buf), "0.1%% Low: %.0f", fps_low(0.001f));
                    pct_line += buf;
                }
                if (state.show_percentile_1) {
                    if (!pct_line.empty()) pct_line += "  ";
                    char buf[32]; snprintf(buf, sizeof(buf), "1%% Low: %.0f", fps_low(0.01f));
                    pct_line += buf;
                }
                if (state.show_percentile_5) {
                    if (!pct_line.empty()) pct_line += "  ";
                    char buf[32]; snprintf(buf, sizeof(buf), "5%% Low: %.0f", fps_low(0.05f));
                    pct_line += buf;
                }
                ImGui::TextColored(TC(ImVec4(0.7f, 0.7f, 0.7f, 1.0f)), "%s", pct_line.c_str());
            }
        }
    }
    ImGui::End();

    ImGui::PopStyleColor(2);
    ImGui::PopStyleVar(2);
}

/* ── reshade_overlay event — fires between NewFrame/EndFrame EVERY frame ─ */

static void on_reshade_overlay(effect_runtime *runtime)
{
    // Try to connect to emu on every frame until successful
    if (!s_bridge_ok) {
        try_connect_bridge();
        if (!s_bridge_ok) return;
    }

    // Store device pointer for icon upload during this frame
    s_current_device = runtime->get_device();
    s_sce_tex_per_frame = 0;  // reset per-frame load cap

    // Refresh appearance values from bridge (overlay config)
    if (s_bridge.GetNotifAppearance) {
        s_bridge.GetNotifAppearance(&s_appearance);
    }

    // ── Detect swapchain colour space and SDR white scale ──
    {
        SwapChainInfo sc = get_swapchain_info(runtime);
        s_addon_cs = sc.cs;
    }
    if (s_bridge.GetSDRWhiteScale) {
        float scale = s_bridge.GetSDRWhiteScale();
        if (scale > 0.01f) s_addon_sdr_scale = scale;
    }
    // Resolve effective colour space (auto-detect + user overrides from config)
    s_addon_ecs = effective_addon_cs();

    // ── Invalidate GPU texture caches if HDR colour space / SDR scale /
    //    image adjustments changed ──
    // Textures have the colour-space transform and image adjustments baked in
    // at upload time.  When any parameter changes, all cached textures must be
    // re-uploaded.
    const bool adj_changed = (fabsf(s_appearance.image_brightness - s_cached_tex_brightness) > 0.001f ||
                              fabsf(s_appearance.image_contrast   - s_cached_tex_contrast)   > 0.001f ||
                              fabsf(s_appearance.image_gamma_adjust - s_cached_tex_gamma_adj) > 0.001f);
    if (s_addon_ecs != s_cached_tex_cs ||
        (s_addon_ecs != SCS_SDR_UNORM && s_addon_ecs != SCS_UNKNOWN &&
         fabsf(s_addon_sdr_scale - s_cached_tex_scale) > 0.01f) ||
        adj_changed)
    {
        auto *data = s_current_device->get_private_data<addon_device_data>();
        if (data) {
            for (auto &[key, icon] : data->icon_cache)
                free_icon(s_current_device, icon);
            data->icon_cache.clear();
            for (auto &[key, icon] : data->avatar_cache)
                free_icon(s_current_device, icon);
            data->avatar_cache.clear();
        }
        s_cached_tex_cs         = s_addon_ecs;
        s_cached_tex_scale      = s_addon_sdr_scale;
        s_cached_tex_brightness = s_appearance.image_brightness;
        s_cached_tex_contrast   = s_appearance.image_contrast;
        s_cached_tex_gamma_adj  = s_appearance.image_gamma_adjust;
    }

    // NOTE: ImGui style colours are NOT patched for HDR — ReShade handles the
    // sRGB→swapchain conversion for ImGui rendering internally.  Only textures
    // need our explicit FP16 HDR transform (via transform_pixels_to_fp16).

    // Flush deferred GPU resource destruction (queued by free_sce_textures last frame)
    {
        auto *data = s_current_device->get_private_data<addon_device_data>();
        if (data && !data->pending_destroy.empty()) {
            for (auto &icon : data->pending_destroy)
                free_icon(s_current_device, icon);
            data->pending_destroy.clear();
        }
    }

    // Get overlay state for timeframe config
    GSE_OverlayState s_frame_state{};
    if (s_bridge.GetState) s_bridge.GetState(&s_frame_state);

    if (s_frame_state.show_fps || s_frame_state.show_frametime) {
        update_fps(s_frame_state.graph_timeframe_sec > 0 ? s_frame_state.graph_timeframe_sec : 5);
    }

    // Stats HUD (always visible when enabled)
    render_stats_hud();

    // Toggle main overlay with our own hotkey (Shift+Tab)
    if (runtime->is_key_down(VK_SHIFT) && runtime->is_key_pressed(VK_TAB)) {
        s_show_main_overlay = !s_show_main_overlay;
        s_overlay_hidden_by_reshade = false;  // explicit toggle overrides any hiding
        // Sync overlay state with the emu DLL so callbacks fire
        if (s_bridge.ShowOverlay)
            s_bridge.ShowOverlay(s_show_main_overlay ? 1 : 0);
    }

    // If ReShade menu was opened while our overlay was visible, temporarily hide ours
    if (s_reshade_menu_open && s_show_main_overlay && !s_overlay_hidden_by_reshade) {
        s_show_main_overlay = false;
        s_overlay_hidden_by_reshade = true;
        if (s_bridge.ShowOverlay)
            s_bridge.ShowOverlay(0);
    }
    // If ReShade menu was closed and we had hidden ours, restore it
    if (!s_reshade_menu_open && s_overlay_hidden_by_reshade) {
        s_show_main_overlay = true;
        s_overlay_hidden_by_reshade = false;
        if (s_bridge.ShowOverlay)
            s_bridge.ShowOverlay(1);
    }

    // Show a software cursor when the GSE overlay is open.
    // ReShade only renders its own cursor when its settings panel is visible,
    // but our overlay needs one too.  Don't touch MouseDrawCursor when the
    // ReShade menu is open — let ReShade manage its own cursor.
    if (!s_reshade_menu_open)
        ImGui::GetIO().MouseDrawCursor = s_show_main_overlay;

    // While the overlay is open, block game input so the mouse and keyboard
    // are routed to ImGui instead of the game.  This also makes ReShade
    // show/update the cursor position even when the game hasn't created one.
    if (s_show_main_overlay)
        runtime->block_input_next_frame();

    // Disable docking for our overlay windows so they can't dock to
    // ReShade panels or to each other.  Save and restore the flag so
    // we don't affect ReShade's own docking behavior.
    ImGuiIO &gse_io = ImGui::GetIO();
    const bool docking_was_enabled = (gse_io.ConfigFlags & ImGuiConfigFlags_DockingEnable) != 0;
    gse_io.ConfigFlags &= ~ImGuiConfigFlags_DockingEnable;

    // Main overlay window
    if (s_show_main_overlay) {
        render_main_overlay(runtime);
    }

    // Notifications rendered LAST so they always draw on top of everything
    render_notifications(runtime);

    // Restore docking state for ReShade's own UI
    if (docking_was_enabled)
        gse_io.ConfigFlags |= ImGuiConfigFlags_DockingEnable;

}

/* ── Main overlay window (rendered when user toggles with Shift+Tab) ──── */

// State for friend context menu
static uint64_t s_ctx_friend_id = 0;

static void render_main_overlay(effect_runtime *runtime)
{
    if (!s_bridge_ok || !s_bridge.GetState) return;

    GSE_OverlayState state{};
    s_bridge.GetState(&state);

    auto &io = ImGui::GetIO();

    // Window title matching native: "Ingame Overlay project - Nemirtingas (Renderer: ReShade Addon)"
    char windowTitle[512];
    snprintf(windowTitle, sizeof(windowTitle),
        "Ingame Overlay project - Nemirtingas (Renderer: ReShade Addon)##gse_main");

    ImGui::SetNextWindowPos(ImVec2(0, 0));
    ImGui::SetNextWindowSize(io.DisplaySize);

    // Apply native style colors
    int style_colors = apply_global_style_colors();

    bool show = true;
    if (!ImGui::Begin(windowTitle, &show,
            ImGuiWindowFlags_NoResize | ImGuiWindowFlags_NoMove | ImGuiWindowFlags_NoCollapse |
            ImGuiWindowFlags_NoBringToFrontOnFocus)) {
        ImGui::End();
        ImGui::PopStyleColor(style_colors);
        if (!show) s_show_main_overlay = false;
        return;
    }
    if (!show) s_show_main_overlay = false;

    // ── User Info (togglable, matching native LabelText format) ──
    if (s_show_user_info) {
        // Show local avatar next to user info
        const float avatar_size = 48.0f;
        const IconTexture *local_avatar = get_or_upload_local_avatar();
        if (local_avatar && local_avatar->valid) {
            ImGui::Image(ImTextureRef(local_avatar->srv.handle), ImVec2(avatar_size, avatar_size));
            ImGui::SameLine();
        }
        ImGui::LabelText("##playinglabel", "%s  (ID: %llu)  playing AppID %u",
            state.username, (unsigned long long)state.steam_id, state.app_id);
        
        // Show lobby/game status
        bool has_lobby = s_bridge.HasLobby ? (s_bridge.HasLobby() != 0) : false;
        if (has_lobby) {
            GSE_LocalLobbyInfo lobby_info{};
            bool got_lobby_info = s_bridge.GetLocalLobbyInfo && s_bridge.GetLocalLobbyInfo(&lobby_info);

            // Get connect string (try from lobby info first, then standalone API)
            char connect_str[GSE_CONNECT_STRING_SIZE] = {};
            bool has_connect_str = false;
            if (got_lobby_info && lobby_info.connect_string[0] != '\0') {
                strncpy(connect_str, lobby_info.connect_string, GSE_CONNECT_STRING_SIZE - 1);
                has_connect_str = true;
            } else if (s_bridge.GetConnectString) {
                has_connect_str = (s_bridge.GetConnectString(connect_str, GSE_CONNECT_STRING_SIZE) != 0);
            }

            // Get exe name for full launch command
            char exe_name[GSE_EXE_NAME_SIZE] = {};
            if (got_lobby_info && lobby_info.exe_name[0] != '\0') {
                strncpy(exe_name, lobby_info.exe_name, GSE_EXE_NAME_SIZE - 1);
            } else if (s_bridge.GetExeName) {
                s_bridge.GetExeName(exe_name, GSE_EXE_NAME_SIZE);
            }

            if (got_lobby_info) {
                // Actual matchmaking lobby
                ImGui::TextColored(TC(ImVec4(0.4f, 0.8f, 0.4f, 1.0f)), "%s (%d/%d) %s",
                    lobby_info.is_owner ? "Has Lobby" : "In Lobby",
                    lobby_info.member_count, lobby_info.member_limit,
                    lobby_info.is_owner ? "[Owner]" : "");
                ImGui::SameLine();
                if (ImGui::SmallButton("Copy Lobby ID")) {
                    char lobby_str[32];
                    snprintf(lobby_str, sizeof(lobby_str), "%llu", (unsigned long long)lobby_info.lobby_id);
                    ImGui::SetClipboardText(lobby_str);
                }
            } else if (has_connect_str) {
                // Connect-string only (no formal lobby, but friends can join)
                ImGui::TextColored(TC(ImVec4(0.4f, 0.8f, 0.4f, 1.0f)), "Hosting Game");
            }

            if (has_connect_str) {
                char launch_cmd[GSE_EXE_NAME_SIZE + GSE_CONNECT_STRING_SIZE + 2] = {};
                if (exe_name[0] != '\0') {
                    snprintf(launch_cmd, sizeof(launch_cmd), "%s %s", exe_name, connect_str);
                } else {
                    snprintf(launch_cmd, sizeof(launch_cmd), "%s", connect_str);
                }
                ImGui::TextColored(TC(ImVec4(0.7f, 0.7f, 0.7f, 1.0f)), "Launch: %s", launch_cmd);
                ImGui::SameLine();
                if (ImGui::SmallButton("Copy##launch")) {
                    ImGui::SetClipboardText(launch_cmd);
                }
            }
        }

        // Show game server info (listen server in same process)
        if (s_bridge.GetGameServerInfo) {
            GSE_GameServerInfo gs_info{};
            if (s_bridge.GetGameServerInfo(&gs_info) && gs_info.active) {
                bool has_name = (gs_info.server_name[0] != '\0');
                bool has_map = (gs_info.map_name[0] != '\0');
                if (has_name || has_map || gs_info.max_players > 0) {
                    char server_line[256] = "Server: ";
                    size_t off = strlen(server_line);
                    if (has_name) {
                        off += snprintf(server_line + off, sizeof(server_line) - off, "%s", gs_info.server_name);
                        if (has_map) off += snprintf(server_line + off, sizeof(server_line) - off, " - %s", gs_info.map_name);
                    } else if (has_map) {
                        off += snprintf(server_line + off, sizeof(server_line) - off, "%s", gs_info.map_name);
                    }
                    if (gs_info.max_players > 0) {
                        snprintf(server_line + off, sizeof(server_line) - off, " (%u/%u)", gs_info.num_players, gs_info.max_players);
                    }
                    ImGui::TextColored(TC(ImVec4(0.6f, 0.8f, 1.0f, 1.0f)), "%s", server_line);
                }
            }
        }
    }

    ImGui::Spacing();

    // ── Button Bar (matching native order exactly) ──
    ImGui::SameLine();
    if (ImGui::Button(translationToggleUserInfo[s_current_language]))
        s_show_user_info = !s_show_user_info;

    // Friends button - show unread indicator if any friend needs attention
    ImGui::SameLine();
    {
        bool has_unread = false;
        for (auto &cw : s_open_chats) {
            GSE_ChatState cs{};
            if (s_bridge.GetChatState && s_bridge.GetChatState(cw.steam_id, &cs) && cs.needs_attention)
                has_unread = true;
        }
        if (has_unread)
            ImGui::PushStyleColor(ImGuiCol_Text, TC(ImVec4(1.0f, 0.8f, 0.2f, 1.0f)));
        if (ImGui::Button(translationFriends[s_current_language]))
            s_show_friends = !s_show_friends;
        if (has_unread)
            ImGui::PopStyleColor();
    }

    ImGui::SameLine();
    if (ImGui::Button(translationShowAchievements[s_current_language]))
        s_show_achievements = !s_show_achievements;

    ImGui::SameLine();
    if (ImGui::Button(translationCopyId[s_current_language])) {
        char id_str[32];
        snprintf(id_str, sizeof(id_str), "%llu", (unsigned long long)state.steam_id);
        ImGui::SetClipboardText(id_str);
    }

    ImGui::SameLine();
    if (ImGui::Button(translationSettings[s_current_language]))
        s_show_settings = !s_show_settings;

    ImGui::SameLine();
    if (ImGui::Button("Networks"))
        s_show_networks = !s_show_networks;

    // Lobby Chat button — only shown when in a lobby
    {
        bool has_lobby = s_bridge.HasLobby ? (s_bridge.HasLobby() != 0) : false;
        if (has_lobby) {
            ImGui::SameLine();
            if (ImGui::Button("Lobby Chat"))
                s_show_lobby_chat = !s_show_lobby_chat;
        } else {
            s_show_lobby_chat = false;
        }
    }

    // ── SCE buttons (matching native: only shown when catalog data is present) ──
    if (s_bridge.GetSceStatus) {
        GSE_SceStatus sce{};
        s_bridge.GetSceStatus(&sce);

        if (sce.data_available) {
            ImGui::SameLine();
            if (!sce.downloading) {
                if (ImGui::Button("Download SCE Assets") && s_bridge.RequestSceDownload)
                    s_bridge.RequestSceDownload();
                ImGui::SameLine();
                if (ImGui::Button(s_show_sce_browser ? "[SCE Assets]" : "Browse SCE Assets"))
                    s_show_sce_browser = !s_show_sce_browser;
            } else {
                ImGui::BeginDisabled();
                ImGui::Button("Downloading SCE Assets...");
                ImGui::EndDisabled();
            }

            // ── SCE Download Progress floating window (matching native: 25% wide, centered) ──
            if (sce.downloading) {
                float win_w = io.DisplaySize.x * 0.25f;
                ImGui::SetNextWindowSize(ImVec2(win_w, 0.0f), ImGuiCond_Always);
                ImGui::SetNextWindowPos(
                    ImVec2(io.DisplaySize.x * 0.5f - win_w * 0.5f, io.DisplaySize.y * 0.08f),
                    ImGuiCond_Always);
                ImGui::SetNextWindowBgAlpha(0.92f);
                ImGui::Begin("SCE Download Progress##sce_dl", nullptr,
                    ImGuiWindowFlags_NoResize | ImGuiWindowFlags_NoMove |
                    ImGuiWindowFlags_NoCollapse | ImGuiWindowFlags_NoSavedSettings |
                    ImGuiWindowFlags_AlwaysAutoResize);

                if (sce.grand_total > 0) {
                    uint32_t grand_done = sce.grand_downloaded + sce.grand_skipped;
                    char total_lbl[128]{};
                    snprintf(total_lbl, sizeof(total_lbl),
                        "Total: %u downloaded, %u cached of %u",
                        sce.grand_downloaded, sce.grand_skipped, sce.grand_total);
                    float frac = (float)grand_done / (float)sce.grand_total;
                    ImGui::ProgressBar(frac, ImVec2(-1.0f, 0.0f), total_lbl);
                } else {
                    ImGui::TextUnformatted("Collecting asset URLs...");
                }

                ImGui::Spacing();
                for (int i = 0; i < GSE_SCE_NUM_TYPES; ++i) {
                    uint32_t tot = sce.types[i].total;
                    if (tot == 0) continue;
                    uint32_t cur = sce.types[i].current;
                    uint32_t dl  = sce.types[i].downloaded;
                    const char *label = sce.type_labels[i] ? sce.type_labels[i] : "Unknown";
                    char lbl[128]{};
                    snprintf(lbl, sizeof(lbl), "%s: %u of %u (%u new)",
                        label, cur, tot, dl);
                    ImGui::ProgressBar((float)cur / (float)tot, ImVec2(-1.0f, 0.0f), lbl);
                }

                ImGui::End();
            }
        }
    }

    // Stats Settings button (replaces individual FPS/Frametime/Playtime checkboxes)
    static bool s_show_stats_settings = false;
    ImGui::Spacing(); ImGui::Spacing();
    ImGui::SameLine();
    if (ImGui::Button("Stats Settings"))
        s_show_stats_settings = !s_show_stats_settings;

    if (s_show_stats_settings && s_bridge.SetOption) {
        ImGui::SetNextWindowSize(ImVec2(310.0f, 0.0f), ImGuiCond_FirstUseEver);
        ImGui::SetNextWindowBgAlpha(0.95f);
        if (ImGui::Begin("Performance Stats Settings##addon", &s_show_stats_settings,
                ImGuiWindowFlags_NoCollapse | ImGuiWindowFlags_NoSavedSettings |
                ImGuiWindowFlags_AlwaysAutoResize)) {

            // --- Master toggles ---
            ImGui::TextColored(TC(ImVec4(0.8f, 0.8f, 0.4f, 1.0f)), "Display");
            ImGui::Separator();
            {
                bool fps_on = state.show_fps != 0;
                bool ft_on  = state.show_frametime != 0;
                bool pt_on  = state.show_playtime != 0;
                if (ImGui::Checkbox("FPS##master", &fps_on))
                    s_bridge.SetOption(GSE_OPT_SHOW_FPS, fps_on ? 1 : 0);
                ImGui::SameLine();
                if (ImGui::Checkbox("Frametime##master", &ft_on))
                    s_bridge.SetOption(GSE_OPT_SHOW_FRAMETIME, ft_on ? 1 : 0);
                ImGui::SameLine();
                if (ImGui::Checkbox("Playtime##master", &pt_on))
                    s_bridge.SetOption(GSE_OPT_SHOW_PLAYTIME, pt_on ? 1 : 0);
            }

            ImGui::Spacing();

            // --- Graph toggles ---
            ImGui::TextColored(TC(ImVec4(0.8f, 0.8f, 0.4f, 1.0f)), "Graphs");
            ImGui::Separator();
            {
                bool fg = state.show_fps_graph != 0;
                bool ftg = state.show_frametime_graph != 0;
                if (ImGui::Checkbox("FPS Graph", &fg))
                    s_bridge.SetOption(GSE_OPT_SHOW_FPS_GRAPH, fg ? 1 : 0);
                ImGui::SameLine();
                if (ImGui::Checkbox("Frametime Graph", &ftg))
                    s_bridge.SetOption(GSE_OPT_SHOW_FRAMETIME_GRAPH, ftg ? 1 : 0);
            }

            ImGui::Spacing();

            // --- Graph timeframe ---
            ImGui::TextColored(TC(ImVec4(0.8f, 0.8f, 0.4f, 1.0f)), "Graph Timeframe");
            ImGui::Separator();
            {
                int tf = state.graph_timeframe_sec > 0 ? state.graph_timeframe_sec : 5;
                if (ImGui::SliderInt("##timeframe", &tf, 1, 30, "%d sec"))
                    s_bridge.SetOption(GSE_OPT_GRAPH_TIMEFRAME_SEC, tf);
            }

            ImGui::Spacing();

            // --- Statistics toggles ---
            ImGui::TextColored(TC(ImVec4(0.8f, 0.8f, 0.4f, 1.0f)), "Statistics");
            ImGui::Separator();
            {
                bool mma = state.show_min_max_avg != 0;
                bool p01 = state.show_percentile_01 != 0;
                bool p1  = state.show_percentile_1 != 0;
                bool p5  = state.show_percentile_5 != 0;
                if (ImGui::Checkbox("Min / Max / Avg", &mma))
                    s_bridge.SetOption(GSE_OPT_SHOW_MIN_MAX_AVG, mma ? 1 : 0);
                if (ImGui::Checkbox("0.1% Low", &p01))
                    s_bridge.SetOption(GSE_OPT_SHOW_PERCENTILE_01, p01 ? 1 : 0);
                ImGui::SameLine();
                if (ImGui::Checkbox("1% Low", &p1))
                    s_bridge.SetOption(GSE_OPT_SHOW_PERCENTILE_1, p1 ? 1 : 0);
                ImGui::SameLine();
                if (ImGui::Checkbox("5% Low", &p5))
                    s_bridge.SetOption(GSE_OPT_SHOW_PERCENTILE_5, p5 ? 1 : 0);
            }
        }
        ImGui::End();
    }

    // ── Rendering Info Panel (matching native layout) ──
    ImGui::Spacing();
    ImGui::Separator();
    {
        // Swap chain info (queried directly from ReShade)
        SwapChainInfo sc_info = get_swapchain_info(runtime);
        ImGui::TextDisabled("API        : ReShade Addon  |  %s", sc_info.api_str);
        ImGui::TextDisabled("Emu build  : %s", state.build_string);
        ImGui::TextDisabled("Build date : %s", state.build_date);

        ImGui::TextDisabled("Swapchain  : %s", sc_info.format_str);
        ImGui::TextDisabled("           : %s", sc_info.type_str);
        
        // sRGB / colour-space correction status (mirrors native overlay logic)
        {
            const char* corr_str;
            const char* corr_reason;
            if (s_appearance.image_gamma == 1) { // SrgbDecode::Enabled (forced on)
                corr_str    = "ON  (forced)";
                corr_reason = "Image_Gamma=on in config";
            } else if (s_appearance.image_gamma == 2) { // SrgbDecode::Disabled (forced off)
                corr_str    = "OFF (forced)";
                corr_reason = "Image_Gamma=off in config";
            } else {
                switch (s_addon_ecs) {
                    case SCS_LINEAR_HDR:
                        corr_str    = "ON ";
                        corr_reason = "linear HDR (FP16/FP32/UNORM16) \xE2\x80\x94 sRGB\xE2\x86\x92linear + SDR-white scale";
                        break;
                    case SCS_HDR10_PQ:
                        corr_str    = "ON ";
                        corr_reason = "HDR10 PQ \xE2\x80\x94 sRGB\xE2\x86\x92linear\xE2\x86\x92PQ (ST.2084) encode";
                        break;
                    case SCS_SDR_SRGB_RTV:
                        corr_str    = "ON ";
                        corr_reason = "_SRGB back-buffer \xE2\x80\x94 sRGB\xE2\x86\x92linear (hw re-encodes)";
                        break;
                    case SCS_SDR_UNORM:
                        corr_str    = "OFF";
                        corr_reason = "SDR UNORM \xE2\x80\x94 bytes pass through unchanged";
                        break;
                    default:
                        corr_str    = "OFF";
                        corr_reason = "awaiting swap chain format detection";
                        break;
                }
            }
            ImGui::TextDisabled("sRGB corr. : %s  \xE2\x80\x94 %s", corr_str, corr_reason);
            if (s_appearance.swapchain_override > 0) {
                static const char* ov_names[] = { "auto", "linear_hdr", "hdr10_pq", "srgb_rtv", "sdr" };
                int idx = s_appearance.swapchain_override;
                const char* ov_name = (idx >= 1 && idx <= 4) ? ov_names[idx] : "?";
                ImGui::TextDisabled("           : Swapchain_Override=%s (user config)", ov_name);
            }
        }

        // Per-display info
        if (s_bridge.GetDisplayInfo) {
            GSE_DisplayInfo displays[4];
            int dcount = s_bridge.GetDisplayInfo(displays, 4);
            if (dcount == 0) {
                ImGui::TextDisabled("Display    : N/A");
            } else {
                for (int d = 0; d < dcount; ++d) {
                    auto &di = displays[d];
                    const char *hdr_st;
                    if (!di.hdr_supported)       hdr_st = "SDR only";
                    else if (di.force_disabled)  hdr_st = "HDR suppressed";
                    else if (di.wide_color && !di.hdr_enabled) hdr_st = "WCG (no HDR)";
                    else if (di.hdr_enabled)     hdr_st = "HDR ON";
                    else                         hdr_st = "HDR OFF (supported)";
                    char bpc_buf[8] = "?";
                    if (di.bpc > 0) snprintf(bpc_buf, sizeof(bpc_buf), "%d", di.bpc);
                    char white_buf[24] = "?";
                    if (di.sdr_white_nits >= 0)
                        snprintf(white_buf, sizeof(white_buf), "%d nits", di.sdr_white_nits);
                    ImGui::TextDisabled("Display %d  : %s  |  %s  |  %sbpc  |  SDR white: %s",
                        d + 1, di.name, hdr_st, bpc_buf, white_buf);
                    ImGui::TextDisabled("           : gamut: %s  |  TF: %s  |  range: %s  |  enc: %s",
                        di.gamut, di.transfer, di.range, di.encoding);
                }
            }
        } else {
            ImGui::TextDisabled("Display    : N/A");
        }

        if (ImGui::SmallButton("Refresh##hdr_info")) {
            // Force re-detection next frame (display info is already live-queried)
            s_addon_cs  = SCS_UNKNOWN;
            s_addon_ecs = SCS_UNKNOWN;
        }

        if (s_bridge.GetSDRWhiteScale) {
            float scale = s_bridge.GetSDRWhiteScale();
            if (s_addon_ecs == SCS_LINEAR_HDR || s_addon_ecs == SCS_HDR10_PQ)
                ImGui::TextDisabled("HDR scale  : %.2fx  (SDR white = %d nits)", scale, (int)(scale * 80.f + 0.5f));
        }

        // -- Image adjustments (live sliders) --
        ImGui::Spacing();
        ImGui::TextDisabled("Image Adjustments:");
        {
            static bool s_adj_slider_active = false;
            bool any_active = false;
            bool released = false;

            float &br = s_appearance.image_brightness;
            float &ct = s_appearance.image_contrast;
            float &ga = s_appearance.image_gamma_adjust;

            ImGui::SliderFloat("Brightness##img", &br, 0.5f, 2.0f, "%.2f");
            if (ImGui::IsItemActive()) any_active = true;
            if (ImGui::IsItemDeactivatedAfterEdit()) released = true;

            ImGui::SliderFloat("Contrast##img",   &ct, 0.5f, 2.0f, "%.2f");
            if (ImGui::IsItemActive()) any_active = true;
            if (ImGui::IsItemDeactivatedAfterEdit()) released = true;

            ImGui::SliderFloat("Gamma##img",      &ga, 0.5f, 2.0f, "%.2f");
            if (ImGui::IsItemActive()) any_active = true;
            if (ImGui::IsItemDeactivatedAfterEdit()) released = true;

            if (any_active) s_adj_slider_active = true;
            if (released)   s_adj_slider_active = false;

            if (ImGui::SmallButton("Reset##img_adj")) {
                br = 1.0f; ct = 1.0f; ga = 1.0f;
                released = true;
            }

            // While dragging, keep cached values in sync to suppress per-frame
            // cache invalidation.  On release / Reset, force a mismatch.
            if (s_adj_slider_active) {
                s_cached_tex_brightness = s_appearance.image_brightness;
                s_cached_tex_contrast   = s_appearance.image_contrast;
                s_cached_tex_gamma_adj  = s_appearance.image_gamma_adjust;
            }
            if (released) {
                s_cached_tex_brightness = -999.0f; // sentinel → forces adj_changed
            }
        }
    }
    ImGui::Separator();

    ImGui::End();
    ImGui::PopStyleColor(style_colors);

    // ── Friends Window (separate Begin(), matching native) ──
    if (s_show_friends) {
        const float min_w = io.DisplaySize.x * 0.25f;
        ImGui::SetNextWindowSizeConstraints(ImVec2(min_w, ImGui::GetFontSize() * 16), ImVec2(8192, 8192));
        ImGui::SetNextWindowBgAlpha(1.0f);
        if (ImGui::Begin(translationFriends[s_current_language], &s_show_friends)) {
            // ---- Local user header: 64px avatar + 3 info lines ----
            {
                const float avatar_size = 64.0f;
                const IconTexture *local_avatar = get_or_upload_local_avatar();
                if (local_avatar && local_avatar->valid) {
                    ImGui::Image(ImTextureRef(local_avatar->srv.handle), ImVec2(avatar_size, avatar_size));
                } else {
                    ImVec2 p = ImGui::GetCursorScreenPos();
                    ImGui::GetWindowDrawList()->AddRectFilled(p, ImVec2(p.x + avatar_size, p.y + avatar_size), TC32(IM_COL32(60, 60, 80, 255)));
                    ImGui::Dummy(ImVec2(avatar_size, avatar_size));
                }
                ImGui::SameLine();

                ImVec2 text_start = ImGui::GetCursorPos();

                // Line 1: Username (ID: steamid)
                ImGui::SetCursorPos(text_start);
                ImGui::TextColored(TC(ImVec4(0.9f, 0.9f, 0.2f, 1.0f)), "%s", state.username);
                ImGui::SameLine();
                ImGui::TextColored(TC(ImVec4(0.6f, 0.6f, 0.6f, 1.0f)), "(ID: %llu)",
                    (unsigned long long)state.steam_id);
                // Show local IPs if available (one line per adapter)
                if (s_bridge.GetLocalIP) {
                    char local_ip[512] = {};
                    if (s_bridge.GetLocalIP(local_ip, sizeof(local_ip)) && local_ip[0]) {
                        // Parse newline-separated "AdapterName: IP" lines
                        char *line = local_ip;
                        while (line && *line) {
                            char *nl = strchr(line, '\n');
                            if (nl) *nl = '\0';
                            ImGui::TextColored(TC(ImVec4(0.5f, 0.7f, 0.9f, 1.0f)), "%s", line);
                            line = nl ? nl + 1 : nullptr;
                        }
                    }
                }

                // Line 2: Playing AppName (AppID XXXX)
                ImGui::SetCursorPosX(text_start.x);
                if (state.app_name[0])
                    ImGui::TextColored(TC(ImVec4(0.5f, 0.8f, 0.5f, 1.0f)), "Playing %s (AppID %u)", state.app_name, state.app_id);
                else
                    ImGui::TextColored(TC(ImVec4(0.5f, 0.8f, 0.5f, 1.0f)), "Playing AppID %u", state.app_id);

                // Line 3: Status - In Game / In Lobby / In Server
                ImGui::SetCursorPosX(text_start.x);
                bool has_lobby = s_bridge.HasLobby ? (s_bridge.HasLobby() != 0) : false;
                GSE_LocalLobbyInfo lobby_info{};
                bool got_lobby_info = has_lobby && s_bridge.GetLocalLobbyInfo && s_bridge.GetLocalLobbyInfo(&lobby_info);

                GSE_GameServerInfo gs_info{};
                bool in_server = s_bridge.GetGameServerInfo && s_bridge.GetGameServerInfo(&gs_info) && gs_info.active;

                if (got_lobby_info) {
                    // Find lobby owner name from friends
                    std::string owner_name;
                    if (lobby_info.is_owner) {
                        owner_name = state.username;
                    } else if (s_bridge.GetFriendCount && s_bridge.GetFriends) {
                        int fc = s_bridge.GetFriendCount();
                        if (fc > 0) {
                            std::vector<GSE_Friend> fl(fc > 256 ? 256 : fc);
                            int cnt = s_bridge.GetFriends(fl.data(), (int)fl.size());
                            for (int i = 0; i < cnt; ++i) {
                                if (fl[i].steam_id == lobby_info.lobby_owner) {
                                    owner_name = fl[i].name;
                                    break;
                                }
                            }
                        }
                    }
                    bool local_is_owner = lobby_info.is_owner;
                    if (!owner_name.empty())
                        ImGui::TextColored(TC(ImVec4(0.4f, 0.8f, 0.4f, 1.0f)), "%s - %llu (%d/%d - %s)",
                            local_is_owner ? "Has Lobby" : "In Lobby",
                            (unsigned long long)lobby_info.lobby_id, lobby_info.member_count, lobby_info.member_limit, owner_name.c_str());
                    else
                        ImGui::TextColored(TC(ImVec4(0.4f, 0.8f, 0.4f, 1.0f)), "%s - %llu (%d/%d)",
                            local_is_owner ? "Has Lobby" : "In Lobby",
                            (unsigned long long)lobby_info.lobby_id, lobby_info.member_count, lobby_info.member_limit);
                } else if (in_server) {
                    if (gs_info.server_name[0] != '\0')
                        ImGui::TextColored(TC(ImVec4(0.6f, 0.8f, 1.0f, 1.0f)), "In Server - %s", gs_info.server_name);
                    else
                        ImGui::TextColored(TC(ImVec4(0.6f, 0.8f, 1.0f, 1.0f)), "In Server");
                } else {
                    ImGui::TextColored(TC(ImVec4(0.5f, 0.7f, 0.5f, 1.0f)), "In Game");
                }
            }

            // ---- Dynamic action buttons ----
            {
                bool has_lobby = s_bridge.HasLobby ? (s_bridge.HasLobby() != 0) : false;
                GSE_LocalLobbyInfo lobby_info{};
                bool got_lobby_early = has_lobby && s_bridge.GetLocalLobbyInfo && s_bridge.GetLocalLobbyInfo(&lobby_info);
                if (has_lobby && got_lobby_early && lobby_info.is_owner && s_bridge.InviteAllFriends) {
                    char invite_all_btn[128];
                    snprintf(invite_all_btn, sizeof(invite_all_btn), "%s##PopupInviteAllFriends_fl", translationInviteAll[s_current_language]);
                    if (ImGui::Button(invite_all_btn)) {
                        s_bridge.InviteAllFriends();
                    }
                    ImGui::SameLine();
                }
                bool got_lobby = got_lobby_early;
                if (got_lobby) {
                    if (ImGui::Button("Copy Lobby ID##fl")) {
                        char lobby_str[32];
                        snprintf(lobby_str, sizeof(lobby_str), "%llu", (unsigned long long)lobby_info.lobby_id);
                        ImGui::SetClipboardText(lobby_str);
                    }
                    ImGui::SameLine();
                    // "Remove All from Lobby" — owner only, more than 1 member
                    if (lobby_info.is_owner && lobby_info.member_count > 1 && s_bridge.KickAllLobbyMembers) {
                        if (ImGui::Button("Remove All from Lobby##fl")) {
                            s_bridge.KickAllLobbyMembers();
                        }
                        ImGui::SameLine();
                    } else if (!lobby_info.is_owner && s_bridge.LeaveLobby) {
                        if (ImGui::Button("Leave Lobby##fl")) {
                            s_bridge.LeaveLobby();
                        }
                        ImGui::SameLine();
                    }
                }
                if (ImGui::Button(translationCopyId[s_current_language])) {
                    char id_str[32];
                    snprintf(id_str, sizeof(id_str), "%llu", (unsigned long long)state.steam_id);
                    ImGui::SetClipboardText(id_str);
                }
                if (s_bridge.GetLocalIP) {
                    char local_ip[512] = {};
                    if (s_bridge.GetLocalIP(local_ip, sizeof(local_ip)) && local_ip[0]) {
                        // Check if multiple adapters (contains newline)
                        if (strchr(local_ip, '\n')) {
                            ImGui::SameLine();
                            if (ImGui::BeginMenu("Copy IP##local")) {
                                char tmp[512];
                                strncpy(tmp, local_ip, sizeof(tmp) - 1);
                                tmp[sizeof(tmp) - 1] = '\0';
                                // Parse newline-separated "AdapterName: IP" lines
                                char *line = tmp;
                                while (line && *line) {
                                    char *nl = strchr(line, '\n');
                                    if (nl) *nl = '\0';
                                    // Extract just the IP part after ": " for clipboard
                                    char *colon = strstr(line, ": ");
                                    const char *ip_only = colon ? colon + 2 : line;
                                    // Show full "AdapterName: IP" as label
                                    if (ImGui::MenuItem(line)) {
                                        ImGui::SetClipboardText(ip_only);
                                    }
                                    line = nl ? nl + 1 : nullptr;
                                }
                                ImGui::EndMenu();
                            }
                        } else {
                            ImGui::SameLine();
                            // Single adapter — extract IP after ": "
                            char *colon = strstr(local_ip, ": ");
                            const char *ip_only = colon ? colon + 2 : local_ip;
                            if (ImGui::Button("Copy IP##local")) {
                                ImGui::SetClipboardText(ip_only);
                            }
                        }
                    }
                }
            }
            ImGui::Separator();

            render_friends_list();
        }
        ImGui::End();
    }

    // ── Settings Window (separate Begin(), matching native) ──
    if (s_show_settings) {
        ImGui::SetNextWindowBgAlpha(1.0f);
        char settings_title[256];
        snprintf(settings_title, sizeof(settings_title), "%s##gse_settings", translationGlobalSettingsWindow[s_current_language]);
        if (ImGui::Begin(settings_title, &s_show_settings)) {
            ImGui::Text("%s", translationGlobalSettingsWindowDescription[s_current_language]);
            ImGui::Separator();

            if (s_bridge.GetOption && s_bridge.SetOption) {
                bool ach_notif = s_bridge.GetOption(GSE_OPT_ACH_NOTIF_ENABLE) != 0;
                if (ImGui::Checkbox("Achievement notifications", &ach_notif))
                    s_bridge.SetOption(GSE_OPT_ACH_NOTIF_ENABLE, ach_notif);

                bool prog_notif = s_bridge.GetOption(GSE_OPT_ACH_PROGRESS_NOTIF_ENABLE) != 0;
                if (ImGui::Checkbox("Achievement progress notifications", &prog_notif))
                    s_bridge.SetOption(GSE_OPT_ACH_PROGRESS_NOTIF_ENABLE, prog_notif);

                bool friend_notif = s_bridge.GetOption(GSE_OPT_FRIEND_NOTIF_ENABLE) != 0;
                if (ImGui::Checkbox("Friend notifications", &friend_notif))
                    s_bridge.SetOption(GSE_OPT_FRIEND_NOTIF_ENABLE, friend_notif);

                ImGui::Separator();

                bool dis_all_warn = s_bridge.GetOption(GSE_OPT_DISABLE_ALL_WARNINGS) != 0;
                if (ImGui::Checkbox("Disable all warnings", &dis_all_warn))
                    s_bridge.SetOption(GSE_OPT_DISABLE_ALL_WARNINGS, dis_all_warn);

                bool dis_bad_appid = s_bridge.GetOption(GSE_OPT_DISABLE_BAD_APPID_WARNING) != 0;
                if (ImGui::Checkbox("Disable bad AppID warning", &dis_bad_appid))
                    s_bridge.SetOption(GSE_OPT_DISABLE_BAD_APPID_WARNING, dis_bad_appid);

                bool dis_local_save = s_bridge.GetOption(GSE_OPT_DISABLE_LOCAL_SAVE_WARNING) != 0;
                if (ImGui::Checkbox("Disable local save warning", &dis_local_save))
                    s_bridge.SetOption(GSE_OPT_DISABLE_LOCAL_SAVE_WARNING, dis_local_save);
            }

            ImGui::Separator();
            ImGui::Text("%s", translationRestartTheGameToApply[s_current_language]);
            if (ImGui::Button(translationSave[s_current_language])) {
                if (s_bridge.SetOption)
                    s_bridge.SetOption(GSE_OPT_DISABLE_ALL_WARNINGS, s_bridge.GetOption(GSE_OPT_DISABLE_ALL_WARNINGS)); // triggers save
                s_show_settings = false;
            }
        }
        ImGui::End();
    }

    // ── Warning Window (separate Begin(), matching native) ──
    {
        bool show_warning = state.warn_local_save || state.warn_bad_appid;
        if (show_warning) {
            ImGui::SetNextWindowSizeConstraints(
                ImVec2(ImGui::GetFontSize() * 32, ImGui::GetFontSize() * 32), ImVec2(8192, 8192));
            ImGui::SetNextWindowFocus();
            ImGui::SetNextWindowBgAlpha(1.0f);
            bool show_win = true;
            char warn_title[256];
            snprintf(warn_title, sizeof(warn_title), "%s##gse_warn", translationWarning[s_current_language]);
            if (ImGui::Begin(warn_title, &show_win)) {
                if (state.warn_bad_appid) {
                    ImGui::TextColored(TC(ImVec4(1, 0, 0, 1)), "WARNING WARNING WARNING");
                    ImGui::TextWrapped("%s", translationWarningDescription_badAppid[s_current_language]);
                    ImGui::TextColored(TC(ImVec4(1, 0, 0, 1)), "WARNING WARNING WARNING");
                }
                if (state.warn_local_save) {
                    ImGui::TextColored(TC(ImVec4(1, 0.8f, 0, 1)), "%s", translationWarningDescription_localSave[s_current_language]);
                }
            }
            ImGui::End();
        }
    }

    // ── Networks Window ──
    if (s_show_networks && s_bridge.GetNetworkInfo) {
        ImGui::SetNextWindowSizeConstraints(ImVec2(ImGui::GetFontSize() * 20, ImGui::GetFontSize() * 12), ImVec2(8192, 8192));
        ImGui::SetNextWindowBgAlpha(1.0f);
        if (ImGui::Begin("Networks", &s_show_networks)) {
            GSE_NetAdapter adapters[8];
            int count = s_bridge.GetNetworkInfo(adapters, 8);

            if (count == 0) {
                ImGui::TextColored(TC(ImVec4(0.5f, 0.5f, 0.5f, 1.0f)), "No network adapters detected");
            }

            for (int ai = 0; ai < count; ++ai) {
                auto &a = adapters[ai];
                char header[256];
                if (a.subnet_str[0])
                    snprintf(header, sizeof(header), "%s (%s) - %s", a.name, a.ip_str, a.subnet_str);
                else
                    snprintf(header, sizeof(header), "%s", a.name);

                if (ImGui::CollapsingHeader(header, ImGuiTreeNodeFlags_DefaultOpen)) {
                    if (a.range_str[0]) {
                        ImGui::TextColored(TC(ImVec4(0.6f, 0.6f, 0.6f, 1.0f)), "  Range: %s", a.range_str);
                    }
                    for (int ui = 0; ui < a.user_count; ++ui) {
                        auto &u = a.users[ui];
                        ImGui::PushID(ai * 100 + ui);

                        if (u.is_self) {
                            ImGui::TextColored(TC(ImVec4(0.5f, 0.7f, 0.9f, 1.0f)), u8"  \u25CF %s", u.name);
                            ImGui::SameLine();
                            ImGui::TextColored(TC(ImVec4(0.5f, 0.7f, 0.9f, 1.0f)), "(%s)", u.ip_str);
                            ImGui::SameLine();
                            ImGui::TextColored(TC(ImVec4(0.6f, 0.6f, 0.6f, 1.0f)), "[You]");
                        } else {
                            ImGui::TextColored(TC(ImVec4(0.9f, 0.9f, 0.2f, 1.0f)), u8"  \u25CF %s", u.name);
                            ImGui::SameLine();
                            ImGui::TextColored(TC(ImVec4(0.5f, 0.7f, 0.9f, 1.0f)), "(%s)", u.ip_str);
                        }
                        // Right-click context menu for copy
                        if (ImGui::IsItemHovered() && ImGui::IsMouseClicked(ImGuiMouseButton_Right))
                            ImGui::OpenPopup("##net_ctx");
                        if (ImGui::BeginPopup("##net_ctx")) {
                            if (ImGui::MenuItem("Copy IP")) {
                                ImGui::SetClipboardText(u.ip_str);
                            }
                            if (ImGui::MenuItem("Copy Name")) {
                                ImGui::SetClipboardText(u.name);
                            }
                            char id_str[32];
                            snprintf(id_str, sizeof(id_str), "%llu", (unsigned long long)u.steam_id);
                            if (ImGui::MenuItem("Copy Steam ID")) {
                                ImGui::SetClipboardText(id_str);
                            }
                            ImGui::EndPopup();
                        }

                        ImGui::PopID();
                    }
                    if (a.user_count == 0) {
                        ImGui::TextColored(TC(ImVec4(0.5f, 0.5f, 0.5f, 1.0f)), "  (no users detected)");
                    }
                }
            }
        }
        ImGui::End();
    }

    // ── Lobby Chat Window ──
    if (s_show_lobby_chat && s_bridge.GetLobbyChatState) {
        ImGui::SetNextWindowSizeConstraints(ImVec2(ImGui::GetFontSize() * 22, ImGui::GetFontSize() * 16), ImVec2(8192, 8192));
        ImGui::SetNextWindowBgAlpha(1.0f);
        if (ImGui::Begin("Lobby Chat", &s_show_lobby_chat)) {
            GSE_LobbyChatState lcs{};
            bool got_state = s_bridge.GetLobbyChatState(&lcs) != 0;

            if (got_state && lcs.lobby_id != 0) {
                // Header: member list
                ImGui::TextColored(TC(ImVec4(0.5f, 0.7f, 0.9f, 1.0f)), "Members (%d):", lcs.member_count);
                ImGui::SameLine();
                for (int i = 0; i < lcs.member_count; ++i) {
                    if (i > 0) ImGui::SameLine();
                    bool is_self = (lcs.members[i].steam_id == state.steam_id);
                    if (is_self)
                        ImGui::TextColored(TC(ImVec4(0.4f, 0.8f, 1.0f, 1.0f)), "%s", lcs.members[i].name);
                    else
                        ImGui::TextColored(TC(ImVec4(0.9f, 0.9f, 0.2f, 1.0f)), "%s", lcs.members[i].name);
                    if (i < lcs.member_count - 1) {
                        ImGui::SameLine();
                        ImGui::TextColored(TC(ImVec4(0.5f, 0.5f, 0.5f, 1.0f)), ",");
                    }
                }
                ImGui::Separator();

                // Chat history area
                float footer_height = ImGui::GetFrameHeightWithSpacing() + 4;
                ImGui::BeginChild("##lobby_chat_history", ImVec2(0, -footer_height), true);

                if (lcs.chat_history[0]) {
                    char *history = lcs.chat_history;
                    char *line = history;
                    while (*line) {
                        char *end = strchr(line, '\n');
                        if (end) *end = '\0';

                        bool is_self_msg = (strncmp(line, "You: ", 5) == 0);
                        if (is_self_msg)
                            ImGui::TextColored(TC(ImVec4(0.4f, 0.8f, 1.0f, 1.0f)), "%s", line);
                        else
                            ImGui::TextColored(TC(ImVec4(0.9f, 0.9f, 0.2f, 1.0f)), "%s", line);

                        if (!end) break;
                        *end = '\n';
                        line = end + 1;
                    }
                } else {
                    ImGui::TextDisabled("No messages yet.");
                }

                // Auto-scroll to bottom when new messages arrive
                static int32_t last_lobby_history_len = 0;
                if (lcs.history_len != last_lobby_history_len) {
                    ImGui::SetScrollHereY(1.0f);
                    last_lobby_history_len = lcs.history_len;
                }

                ImGui::EndChild();

                // Input bar
                static char lobby_input_buf[768] = {};
                float send_btn_w = ImGui::CalcTextSize("Send").x + ImGui::GetStyle().FramePadding.x * 2 + 8;
                ImGui::SetNextItemWidth(ImGui::GetContentRegionAvail().x - send_btn_w - 8);

                bool send_msg = false;
                if (ImGui::InputText("##lobby_chat_input", lobby_input_buf, sizeof(lobby_input_buf),
                        ImGuiInputTextFlags_EnterReturnsTrue)) {
                    send_msg = true;
                }
                ImGui::SameLine();
                if (ImGui::Button("Send##lobby_send") || send_msg) {
                    if (lobby_input_buf[0] && s_bridge.SendLobbyChatMsg) {
                        s_bridge.SendLobbyChatMsg(lobby_input_buf);
                        lobby_input_buf[0] = '\0';
                    }
                }
            } else {
                ImGui::TextColored(TC(ImVec4(0.5f, 0.5f, 0.5f, 1.0f)), "Not in a lobby.");
            }
        }
        ImGui::End();
    }

    // ── Achievement Window (separate Begin(), matching native) ──
    if (s_show_achievements) {
        render_achievement_list();
    }

    // ── Chat Windows (tabbed window) ──
    check_incoming_messages();  // auto-add friends who message us
    render_chat_windows();

    // ── SCE Asset Browser Window (matching native exactly) ──
    if (s_show_sce_browser && s_bridge.GetSceStatus && s_bridge.GetSceSeriesCount) {
        GSE_SceStatus sce{};
        s_bridge.GetSceStatus(&sce);

        if (!sce.data_available) {
            s_show_sce_browser = false;
            free_sce_textures();
        } else {
            // Per-type card dimensions matching native: {card_width, image_height}
            struct CardDims { float cw, ih; };
            static constexpr CardDims kDims[14] = {
                {160.f, 200.f},  // 0  cards                 (portrait)
                {160.f, 200.f},  // 1  foil_cards
                {160.f, 160.f},  // 2  booster_packs          (square)
                {160.f, 160.f},  // 3  badges
                {160.f, 160.f},  // 4  foil_badges
                {160.f, 160.f},  // 5  emoticons
                {220.f, 124.f},  // 6  backgrounds            (16:9)
                {220.f, 124.f},  // 7  animated_backgrounds
                {220.f, 124.f},  // 8  animated_mini_backgrounds
                {160.f, 160.f},  // 9  profiles
                {160.f, 160.f},  // 10 avatar_frames
                {160.f, 160.f},  // 11 animated_avatars
                {160.f, 160.f},  // 12 animated_stickers
                {220.f, 124.f},  // 13 startup_movies         (16:9)
            };

            // Tab grouping matching native
            struct SceTabGroup { const char *label; int types[4]; int ntype; };
            static constexpr SceTabGroup kTabs[] = {
                { "Cards",       { 0,  1,  2, -1}, 3 },
                { "Badges",      { 3,  4, -1, -1}, 2 },
                { "Backgrounds", { 6,  7,  8, -1}, 3 },
                { "Chat",        { 5, 12, -1, -1}, 2 },
                { "Profiles",    {11, 10,  9, -1}, 3 },
                { "Steam",       {13, -1, -1, -1}, 1 },
            };
            constexpr float CARD_GAP = 12.0f;

            auto &io = ImGui::GetIO();
            const float min_w = io.DisplaySize.x * 0.60f;
            ImGui::SetNextWindowSizeConstraints(
                ImVec2(min_w, io.DisplaySize.y * 0.50f),
                ImVec2(8192.0f, 8192.0f));
            ImGui::SetNextWindowBgAlpha(1.0f);

            bool browser_open = s_show_sce_browser;
            if (ImGui::Begin("SCE Assets##sce_browser", &browser_open)) {
                ImGui::TextDisabled("Assets folder: %s", s_sce_storage_path.c_str());
                ImGui::Separator();
                ImGui::Spacing();

                // Get all series
                int ser_count = s_bridge.GetSceSeriesCount();
                std::vector<GSE_SceSeries> series_arr(ser_count > 64 ? 64 : ser_count);
                if (s_bridge.GetSceSeries)
                    ser_count = s_bridge.GetSceSeries(series_arr.data(), (int)series_arr.size());

                for (int si = 0; si < ser_count; ++si) {
                    auto &ser = series_arr[si];

                    // Series header
                    char ser_label[256]{};
                    if (ser.series_name[0])
                        snprintf(ser_label, sizeof(ser_label), "Series %d - %s",
                            ser.series_number, ser.series_name);
                    else
                        snprintf(ser_label, sizeof(ser_label), "Series %d", ser.series_number);

                    // Build series directory name (matching downloader)
                    char ser_num_str[8]{};
                    snprintf(ser_num_str, sizeof(ser_num_str), "%02d", ser.series_number);
                    std::string ser_dir = std::string("Series ") + ser_num_str;
                    if (ser.series_name[0])
                        ser_dir += " - " + sce_sanitize(ser.series_name);

                    if (!ImGui::CollapsingHeader(ser_label, ImGuiTreeNodeFlags_DefaultOpen))
                        continue;

                    // Get all items in this series
                    std::vector<GSE_SceItem> items(ser.item_count > 512 ? 512 : ser.item_count);
                    int item_count = 0;
                    if (s_bridge.GetSceItems)
                        item_count = s_bridge.GetSceItems(ser.series_number, items.data(), (int)items.size());

                    // Group items by type
                    std::unordered_map<int, std::vector<int>> by_type; // type -> item indices
                    for (int i = 0; i < item_count; ++i)
                        by_type[items[i].type].push_back(i);

                    char tab_bar_id[32]{};
                    snprintf(tab_bar_id, sizeof(tab_bar_id), "##tb_%d", ser.series_number);
                    if (!ImGui::BeginTabBar(tab_bar_id)) { ImGui::Spacing(); continue; }

                    for (int tgi = 0; tgi < (int)(sizeof(kTabs)/sizeof(kTabs[0])); ++tgi) {
                        auto &tg = kTabs[tgi];

                        // Count total items for this tab group
                        int tab_total = 0;
                        for (int ti = 0; ti < tg.ntype; ++ti) {
                            auto it2 = by_type.find(tg.types[ti]);
                            if (it2 != by_type.end()) tab_total += (int)it2->second.size();
                        }
                        if (tab_total == 0) continue;

                        char tab_lbl[80]{};
                        snprintf(tab_lbl, sizeof(tab_lbl), "%s (%d)##tb_%d_%d",
                            tg.label, tab_total, ser.series_number, tgi);
                        if (!ImGui::BeginTabItem(tab_lbl)) continue;

                        bool first_type = true;
                        for (int ti = 0; ti < tg.ntype; ++ti) {
                            int tidx = tg.types[ti];
                            auto it3 = by_type.find(tidx);
                            if (it3 == by_type.end() || it3->second.empty()) continue;

                            auto &item_indices = it3->second;
                            float card_w = kDims[tidx].cw;
                            float img_h  = kDims[tidx].ih;

                            // Sub-header per type
                            if (!first_type) ImGui::Spacing();
                            ImGui::TextDisabled("%s  (%zu)",
                                sce.type_labels[tidx] ? sce.type_labels[tidx] : "Unknown",
                                item_indices.size());
                            ImGui::Separator();
                            ImGui::Spacing();
                            first_type = false;

                            // Wrapping card grid
                            float avail_w = ImGui::GetContentRegionAvail().x;
                            int cols = (std::max)(1, (int)((avail_w + CARD_GAP) / (card_w + CARD_GAP)));
                            char tbl_id[32]{};
                            snprintf(tbl_id, sizeof(tbl_id), "##cg_%d_%d", ser.series_number, tidx);
                            if (ImGui::BeginTable(tbl_id, cols,
                                    ImGuiTableFlags_SizingFixedFit | ImGuiTableFlags_NoBordersInBody |
                                    ImGuiTableFlags_NoPadOuterX)) {
                                for (int c = 0; c < cols; ++c)
                                    ImGui::TableSetupColumn("", ImGuiTableColumnFlags_WidthFixed, card_w);

                                int slot_idx = 0;
                                for (int idx : item_indices) {
                                    auto &item = items[idx];
                                    ++slot_idx;
                                    ImGui::TableNextColumn();

                                    bool is_bg = (tidx == 6 || tidx == 7 || tidx == 8);

                                    // Build thumbnail file path (mirrors downloader naming)
                                    const char *thumb_src = is_bg
                                        ? (item.wallpaper_url[0] ? item.wallpaper_url : item.static_img_url)
                                        : (item.icon_url[0] ? item.icon_url : item.static_img_url);
                                    std::string ext = sce_url_ext(thumb_src);
                                    std::string item_label = sce_sanitize(item.name);
                                    char prefix[8]{};
                                    snprintf(prefix, sizeof(prefix), "%02d_", slot_idx);

                                    std::string filename = is_bg
                                        ? (std::string(prefix) + "thumb_wallpaper_" + item_label + ext)
                                        : (std::string(prefix) + "icon_" + item_label + ext);
                                    std::string type_dir = sce_type_subfolder(tidx);
                                    std::string folder = s_sce_storage_path + ser_dir + "\\" + type_dir;
                                    std::string tex_path = folder + "\\" + filename;

                                    bool is_static = (ext != ".gif" && ext != ".mp4" && ext != ".webm");

                                    // Try to load the image texture
                                    const IconTexture *tex = nullptr;
                                    if (is_static)
                                        tex = get_or_load_image(tex_path);

                                    // --- Card ---
                                    ImGui::BeginGroup();

                                    // Slot / total line above image
                                    if (item.slot > 0 && item.total > 0)
                                        ImGui::TextDisabled("#%d of %d", item.slot, item.total);
                                    else if (item.slot > 0)
                                        ImGui::TextDisabled("#%d", item.slot);
                                    else
                                        ImGui::TextDisabled(" ");

                                    // Image area
                                    ImVec2 p0 = ImGui::GetCursorScreenPos();
                                    ImVec2 p1 = ImVec2(p0.x + card_w, p0.y + img_h);
                                    ImDrawList *dl = ImGui::GetWindowDrawList();
                                    dl->AddRectFilled(p0, p1, TC32(IM_COL32(40, 40, 50, 255)));

                                    char btn_id[64]{};
                                    snprintf(btn_id, sizeof(btn_id), "##bg_%d_%d_%d",
                                        ser.series_number, tidx, slot_idx);
                                    ImGui::InvisibleButton(btn_id, ImVec2(card_w, img_h));
                                    bool clicked = ImGui::IsItemClicked();

                                    if (is_static && tex && tex->valid) {
                                        // Aspect-fit (letterbox) image into card_w x img_h
                                        float sa = (tex->h > 0) ? (float)tex->w / tex->h : 1.0f;
                                        float da = card_w / img_h;
                                        float dw, dh, ox = 0.f, oy = 0.f;
                                        if (sa >= da) {
                                            dw = card_w; dh = card_w / sa;
                                            oy = (img_h - dh) * 0.5f;
                                        } else {
                                            dh = img_h; dw = img_h * sa;
                                            ox = (card_w - dw) * 0.5f;
                                        }
                                        dl->AddImage(ImTextureRef(tex->srv.handle),
                                            ImVec2(p0.x + ox, p0.y + oy),
                                            ImVec2(p0.x + ox + dw, p0.y + oy + dh));
                                        // Hover highlight
                                        if (ImGui::IsItemHovered())
                                            dl->AddRect(p0, p1, TC32(IM_COL32(200, 200, 255, 180)), 0.f, 0, 2.f);
                                    } else if (!is_static) {
                                        // Animated/video: show extension badge centred
                                        const char *badge = ext.size() > 1 ? ext.c_str() + 1 : ext.c_str();
                                        ImVec2 tsz = ImGui::CalcTextSize(badge);
                                        dl->AddText(
                                            ImVec2(p0.x + (card_w - tsz.x) * 0.5f,
                                                   p0.y + (img_h  - tsz.y) * 0.5f),
                                            TC32(IM_COL32(160, 160, 160, 255)), badge);
                                    }

                                    // On click: open full-size preview
                                    bool is_animated_bg = (tidx == 7 || tidx == 8);
                                    bool can_preview = is_animated_bg ? (item.wallpaper_url[0] != 0) : is_static;
                                    if (clicked && can_preview) {
                                        if (is_animated_bg) {
                                            std::string full_ext = sce_url_ext(item.wallpaper_url);
                                            std::string full_fn = std::string(prefix) + "wallpaper_" + item_label + full_ext;
                                            s_sce_preview_key = folder + "\\" + full_fn;
                                        } else {
                                            s_sce_preview_key = tex_path;
                                        }
                                        // Build nav keys for same type within this series
                                        s_sce_preview_nav_keys.clear();
                                        int nav_slot = 0;
                                        for (int ni : item_indices) {
                                            ++nav_slot;
                                            auto &nitem = items[ni];
                                            char npfx[8]{};
                                            snprintf(npfx, sizeof(npfx), "%02d_", nav_slot);
                                            std::string nlbl = sce_sanitize(nitem.name);
                                            std::string npreview_file;
                                            if (is_animated_bg) {
                                                if (!nitem.wallpaper_url[0]) continue;
                                                std::string next = sce_url_ext(nitem.wallpaper_url);
                                                npreview_file = std::string(npfx) + "wallpaper_" + nlbl + next;
                                            } else {
                                                const char *nsrc = nitem.icon_url[0] ? nitem.icon_url : nitem.static_img_url;
                                                std::string next = sce_url_ext(nsrc);
                                                npreview_file = std::string(npfx) + "icon_" + nlbl + next;
                                            }
                                            s_sce_preview_nav_keys.push_back(folder + "\\" + npreview_file);
                                        }
                                    }

                                    // Name (wrapped to card width)
                                    ImGui::PushTextWrapPos(ImGui::GetCursorPosX() + card_w);
                                    ImGui::TextUnformatted(item.name);
                                    ImGui::PopTextWrapPos();

                                    // Rarity (coloured)
                                    if (item.rarity[0] && strcmp(item.rarity, "Unknown") != 0)
                                        ImGui::TextColored(sce_rarity_color(item.rarity),
                                            "%s", item.rarity);
                                    else
                                        ImGui::TextDisabled(" ");

                                    // Price
                                    if (item.price_text[0])
                                        ImGui::TextDisabled("%s", item.price_text);
                                    else
                                        ImGui::TextDisabled(" ");

                                    // Badge level / XP (types 3 + 4)
                                    if ((tidx == 3 || tidx == 4) && (item.badge_level > 0 || item.badge_xp > 0)) {
                                        if (item.badge_level > 0 && item.badge_xp > 0)
                                            ImGui::TextDisabled("Lv.%d  %d XP", item.badge_level, item.badge_xp);
                                        else if (item.badge_level > 0)
                                            ImGui::TextDisabled("Lv.%d", item.badge_level);
                                        else
                                            ImGui::TextDisabled("%d XP", item.badge_xp);
                                    }

                                    // Emoticon shortcode (type 5)
                                    if (tidx == 5 && item.emoticon_name[0])
                                        ImGui::TextDisabled(":%s:", item.emoticon_name);

                                    // Steam Points cost
                                    if (item.points_price[0])
                                        ImGui::TextDisabled("Pts: %s", item.points_price);

                                    ImGui::EndGroup();
                                }
                                ImGui::EndTable();
                            }
                            ImGui::Spacing();
                        }

                        ImGui::EndTabItem();
                    }
                    ImGui::EndTabBar();
                    ImGui::Spacing();
                }
            }
            ImGui::End();

            // ── Full-size asset preview (matching native: dimmed bg, centered, prev/next nav) ──
            if (!s_sce_preview_key.empty()) {
                auto &io2 = ImGui::GetIO();

                // Dim everything behind
                ImGui::GetBackgroundDrawList()->AddRectFilled(
                    ImVec2(0, 0), io2.DisplaySize, TC32(IM_COL32(0, 0, 0, 180)));

                // Load the preview image immediately (no frame cap)
                const IconTexture *ftex = load_preview_image(s_sce_preview_key);

                float pw = io2.DisplaySize.x * 0.75f;
                float ph = pw * (9.f / 16.f);
                if (ftex && ftex->valid && ftex->w > 0 && ftex->h > 0) {
                    ph = pw * ((float)ftex->h / ftex->w);
                    float max_h = io2.DisplaySize.y * 0.85f;
                    if (ph > max_h) { ph = max_h; pw = ph * ((float)ftex->w / ftex->h); }
                }
                ImVec2 wpos((io2.DisplaySize.x - pw) * 0.5f,
                            (io2.DisplaySize.y - ph) * 0.5f);
                ImGui::SetNextWindowPos(wpos, ImGuiCond_Always);
                ImGui::SetNextWindowSize(ImVec2(pw, ph), ImGuiCond_Always);
                ImGui::SetNextWindowBgAlpha(0.97f);

                bool preview_open = true;
                if (ImGui::Begin("##sce_preview", &preview_open,
                        ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_NoNav |
                        ImGuiWindowFlags_NoMove | ImGuiWindowFlags_NoSavedSettings)) {
                    constexpr float BTN_H = 28.f;
                    constexpr float BTN_W = 64.f;
                    constexpr float NAV_PAD = 8.f;
                    float img_avail_h = ImGui::GetContentRegionAvail().y - BTN_H - NAV_PAD * 2.f;

                    if (ftex && ftex->valid) {
                        ImVec2 avail(ImGui::GetContentRegionAvail().x, img_avail_h);
                        float sa = (ftex->h > 0) ? (float)ftex->w / ftex->h : 1.0f;
                        float dw = avail.x, dh = avail.x / sa;
                        if (dh > avail.y) { dh = avail.y; dw = avail.y * sa; }
                        float padx = (avail.x - dw) * 0.5f;
                        float pady = (avail.y - dh) * 0.5f;
                        ImGui::SetCursorPos(ImVec2(ImGui::GetCursorPosX() + padx,
                                                   ImGui::GetCursorPosY() + pady));
                        ImGui::Image(ImTextureRef(ftex->srv.handle), ImVec2(dw, dh));
                    } else {
                        ImGui::SetCursorPosY(img_avail_h * 0.45f);
                        ImGui::SetCursorPosX((pw - ImGui::CalcTextSize("Loading...").x) * 0.5f);
                        ImGui::TextDisabled("Loading...");
                    }

                    // Navigation buttons
                    int nav_cur = -1;
                    for (int ni = 0; ni < (int)s_sce_preview_nav_keys.size(); ++ni)
                        if (s_sce_preview_nav_keys[ni] == s_sce_preview_key) { nav_cur = ni; break; }

                    auto nav_to = [&](int ni) { s_sce_preview_key = s_sce_preview_nav_keys[ni]; };

                    bool can_prev = nav_cur > 0;
                    bool can_next = nav_cur >= 0 && nav_cur < (int)s_sce_preview_nav_keys.size() - 1;

                    // Arrow / A-D key navigation
                    if (can_prev && (ImGui::IsKeyPressed(ImGuiKey_LeftArrow) || ImGui::IsKeyPressed(ImGuiKey_A)))
                        nav_to(nav_cur - 1);
                    if (can_next && (ImGui::IsKeyPressed(ImGuiKey_RightArrow) || ImGui::IsKeyPressed(ImGuiKey_D)))
                        nav_to(nav_cur + 1);

                    // Button bar pinned to bottom
                    ImVec2 cpos = ImGui::GetWindowPos();
                    ImVec2 csz  = ImGui::GetWindowSize();
                    float by = cpos.y + csz.y - BTN_H - 6.f;

                    ImGui::SetCursorScreenPos(ImVec2(cpos.x + 6.f, by));
                    ImGui::BeginDisabled(!can_prev);
                    if (ImGui::Button("< Prev", ImVec2(BTN_W, BTN_H)) && can_prev)
                        nav_to(nav_cur - 1);
                    ImGui::EndDisabled();

                    // Counter label centred
                    if (nav_cur >= 0) {
                        char cnt[32]{};
                        snprintf(cnt, sizeof(cnt), "%d / %d",
                            nav_cur + 1, (int)s_sce_preview_nav_keys.size());
                        ImVec2 tsz = ImGui::CalcTextSize(cnt);
                        ImGui::SetCursorScreenPos(ImVec2(
                            cpos.x + (csz.x - tsz.x) * 0.5f,
                            by + (BTN_H - tsz.y) * 0.5f));
                        ImGui::TextDisabled("%s", cnt);
                    }

                    ImGui::SetCursorScreenPos(ImVec2(cpos.x + csz.x - BTN_W - 6.f, by));
                    ImGui::BeginDisabled(!can_next);
                    if (ImGui::Button("Next >", ImVec2(BTN_W, BTN_H)) && can_next)
                        nav_to(nav_cur + 1);
                    ImGui::EndDisabled();

                    // Close: Esc, right-click, or clicking outside
                    if (ImGui::IsKeyPressed(ImGuiKey_Escape) ||
                        ImGui::IsMouseClicked(ImGuiMouseButton_Right) ||
                        (!ImGui::IsWindowHovered(ImGuiHoveredFlags_AnyWindow) &&
                          ImGui::IsMouseClicked(ImGuiMouseButton_Left)))
                        preview_open = false;
                }
                ImGui::End();
                if (!preview_open)
                    s_sce_preview_key.clear();
            }

            // Free textures when browser is closed
            if (!browser_open) {
                s_show_sce_browser = false;
                s_sce_preview_key.clear();
                s_sce_preview_nav_keys.clear();
                free_sce_textures();
            }
        }
    }
}

/* ── Friends list section ─────────────────────────────────────────────── */

static void render_friends_list()
{
    if (!s_bridge.GetFriendCount || !s_bridge.GetFriends) return;

    int friend_count = s_bridge.GetFriendCount();
    if (friend_count <= 0) return;

    std::vector<GSE_Friend> friends(friend_count > 256 ? 256 : friend_count);
    int count = s_bridge.GetFriends(friends.data(), (int)friends.size());

    // Check if local user has a lobby (for individual Invite buttons)
    bool has_lobby = s_bridge.HasLobby ? (s_bridge.HasLobby() != 0) : false;

    // ---- Partition friends into In Game (same app) / Online (different app) ----
    std::vector<int> in_game_idx, online_idx;
    for (int i = 0; i < count; ++i) {
        if (friends[i].same_app)
            in_game_idx.push_back(i);
        else
            online_idx.push_back(i);
    }

    // ---- Per-friend renderer (3-line: avatar + name/id + game + lobby) ----
    auto render_friend_row = [&](int idx) {
        auto &f = friends[idx];
        ImGui::PushID(idx);

        // 48px avatar to fit 3 text lines
        const float avatar_size = 48.0f;
        float line_h = ImGui::GetTextLineHeight();
        float row_height = (std::max)(avatar_size, line_h * 3.0f);
        ImVec2 cursor_before = ImGui::GetCursorPos();
        ImGui::Selectable("##friend_row", false, ImGuiSelectableFlags_AllowOverlap, ImVec2(0, row_height));
        // Capture right-click on the full selectable row for context menu
        if (ImGui::IsItemClicked(ImGuiMouseButton_Right))
            ImGui::OpenPopup("##ctx_friend");
        ImGui::SetCursorPos(cursor_before);

        // Avatar
        const IconTexture *avatar = get_or_upload_avatar(f.steam_id);
        if (avatar && avatar->valid) {
            ImGui::Image(ImTextureRef(avatar->srv.handle), ImVec2(avatar_size, avatar_size));
        } else {
            ImVec2 p = ImGui::GetCursorScreenPos();
            ImGui::GetWindowDrawList()->AddRectFilled(p, ImVec2(p.x + avatar_size, p.y + avatar_size), TC32(IM_COL32(60, 60, 80, 255)));
            ImGui::Dummy(ImVec2(avatar_size, avatar_size));
        }
        ImGui::SameLine();

        ImVec2 text_start = ImGui::GetCursorPos();

        // Line 1: FriendName (ID: steamid)
        ImGui::SetCursorPos(text_start);
        bool needs_attn = (f.window_state & 0x08); // window_state_need_attention
        if (needs_attn)
            ImGui::TextColored(TC(ImVec4(1.0f, 0.8f, 0.2f, 1.0f)), "%s", f.name);
        else
            ImGui::TextColored(TC(ImVec4(0.9f, 0.9f, 0.2f, 1.0f)), "%s", f.name);
        ImGui::SameLine();
        ImGui::TextColored(TC(ImVec4(0.6f, 0.6f, 0.6f, 1.0f)), "(ID: %llu)", (unsigned long long)f.steam_id);
        // Show detected IP if available
        if (f.ip_str[0]) {
            ImGui::SameLine();
            ImGui::TextColored(TC(ImVec4(0.5f, 0.7f, 0.9f, 1.0f)), "[%s]", f.ip_str);
        }

        // Line 2: Playing AppName (AppID XXXX)
        ImGui::SetCursorPosX(text_start.x);
        if (f.appid != 0) {
            if (f.app_name[0])
                ImGui::TextColored(TC(ImVec4(0.5f, 0.8f, 0.5f, 1.0f)), "Playing %s (AppID %u)", f.app_name, f.appid);
            else
                ImGui::TextColored(TC(ImVec4(0.5f, 0.8f, 0.5f, 1.0f)), "Playing AppID %u", f.appid);
        } else {
            ImGui::TextColored(TC(ImVec4(0.5f, 0.5f, 0.5f, 1.0f)), "Online");
        }

        // Line 3: Lobby info (if friend has a lobby)
        if (f.in_lobby && f.lobby_id != 0) {
            ImGui::SetCursorPosX(text_start.x);
            bool frd_is_owner = (f.lobby_owner_name[0] && strcmp(f.lobby_owner_name, f.name) == 0);
            if (f.lobby_owner_name[0])
                ImGui::TextColored(TC(ImVec4(0.4f, 0.8f, 0.4f, 1.0f)), "%s - %llu (%d/%d - %s)",
                    frd_is_owner ? "Has Lobby" : "In Lobby",
                    (unsigned long long)f.lobby_id, f.lobby_member_count, f.lobby_member_limit, f.lobby_owner_name);
            else
                ImGui::TextColored(TC(ImVec4(0.4f, 0.8f, 0.4f, 1.0f)), "In Lobby - %llu (%d/%d)",
                    (unsigned long long)f.lobby_id, f.lobby_member_count, f.lobby_member_limit);
        }

        // Right-click context menu (opened from selectable above)
        if (ImGui::BeginPopup("##ctx_friend")) {
            if (ImGui::MenuItem(translationChat[s_current_language])) {
                // Open chat
                int found_idx = -1;
                for (size_t ci = 0; ci < s_open_chats.size(); ++ci) {
                    if (s_open_chats[ci].steam_id == f.steam_id) { found_idx = (int)ci; break; }
                }
                if (found_idx < 0) {
                    AddonChatWindow cw{};
                    cw.steam_id = f.steam_id;
                    strncpy(cw.friend_name, f.name, sizeof(cw.friend_name) - 1);
                    s_open_chats.push_back(cw);
                    found_idx = (int)s_open_chats.size() - 1;
                }
                s_show_chat = true;
                s_active_chat_idx = found_idx;
                if (s_bridge.OpenChat) s_bridge.OpenChat(f.steam_id);
            }
            bool is_lobby_owner_ctx = false;
            if (has_lobby && s_bridge.GetLocalLobbyInfo) {
                GSE_LocalLobbyInfo linfo_ctx{};
                if (s_bridge.GetLocalLobbyInfo(&linfo_ctx)) is_lobby_owner_ctx = linfo_ctx.is_owner;
            }
            if (has_lobby && is_lobby_owner_ctx && f.same_app && !f.in_my_lobby && s_bridge.FriendAction) {
                if (ImGui::MenuItem(translationInvite[s_current_language])) {
                    s_bridge.FriendAction(f.steam_id, GSE_FRIEND_ACTION_INVITE);
                }
            }
            if (f.same_app && f.is_joinable && f.lobby_id != 0 && !f.in_my_lobby && s_bridge.FriendAction) {
                if (ImGui::MenuItem(translationJoin[s_current_language])) {
                    s_bridge.FriendAction(f.steam_id, GSE_FRIEND_ACTION_JOIN);
                }
            }
            // Show Accept Invite if this friend sent us a pending invite we haven't accepted yet
            if (f.has_pending_invite && s_bridge.FriendAction) {
                if (ImGui::MenuItem("Accept Invite")) {
                    s_bridge.FriendAction(f.steam_id, GSE_FRIEND_ACTION_ACCEPT_INVITE);
                }
            }
            if (ImGui::MenuItem(translationCopyId[s_current_language])) {
                char id_str[32];
                snprintf(id_str, sizeof(id_str), "%llu", (unsigned long long)f.steam_id);
                ImGui::SetClipboardText(id_str);
            }
            if (f.ip_str[0]) {
                // Check if there are multiple IPs (contains comma)
                if (strchr(f.ip_str, ',')) {
                    if (ImGui::BeginMenu("Copy IP")) {
                        // Parse comma-separated IPs
                        char tmp[256];
                        strncpy(tmp, f.ip_str, sizeof(tmp) - 1);
                        tmp[sizeof(tmp) - 1] = '\0';
                        char *ctx = nullptr;
                        char *tok = strtok_s(tmp, ",", &ctx);
                        while (tok) {
                            while (*tok == ' ') ++tok; // trim leading space
                            if (ImGui::MenuItem(tok)) {
                                ImGui::SetClipboardText(tok);
                            }
                            tok = strtok_s(nullptr, ",", &ctx);
                        }
                        ImGui::EndMenu();
                    }
                } else {
                    if (ImGui::MenuItem("Copy IP")) {
                        ImGui::SetClipboardText(f.ip_str);
                    }
                }
            }
            if (f.lobby_id != 0) {
                if (ImGui::MenuItem("Copy Lobby ID")) {
                    char lobby_str[32];
                    snprintf(lobby_str, sizeof(lobby_str), "%llu", (unsigned long long)f.lobby_id);
                    ImGui::SetClipboardText(lobby_str);
                }
            }
            // Kick from lobby (only if friend is in our lobby and we're the owner)
            if (f.in_my_lobby && s_bridge.FriendAction && s_bridge.GetLocalLobbyInfo) {
                GSE_LocalLobbyInfo linfo{};
                if (s_bridge.GetLocalLobbyInfo(&linfo) && linfo.is_owner) {
                    ImGui::Separator();
                    if (ImGui::MenuItem("Kick from Lobby")) {
                        s_bridge.FriendAction(f.steam_id, GSE_FRIEND_ACTION_KICK);
                    }
                }
            }
            ImGui::EndPopup();
        }

        ImGui::PopID();
    };

    // ---- Render grouped friend list ----
    ImGui::BeginChild("##friends_child", ImVec2(0, 0), true);

    // In Game section
    if (!in_game_idx.empty()) {
        char hdr[64];
        snprintf(hdr, sizeof(hdr), "In Game (%d)##frd_ingame", (int)in_game_idx.size());
        ImGui::PushStyleColor(ImGuiCol_Header, TC(ImVec4(0.15f, 0.35f, 0.15f, 0.80f)));
        ImGui::PushStyleColor(ImGuiCol_HeaderHovered, TC(ImVec4(0.20f, 0.45f, 0.20f, 0.90f)));
        ImGui::PushStyleColor(ImGuiCol_HeaderActive, TC(ImVec4(0.25f, 0.55f, 0.25f, 1.00f)));
        bool open = ImGui::CollapsingHeader(hdr, ImGuiTreeNodeFlags_DefaultOpen);
        ImGui::PopStyleColor(3);
        if (open) {
            for (int idx : in_game_idx) render_friend_row(idx);
        }
    }

    // Online section
    if (!online_idx.empty()) {
        char hdr[64];
        snprintf(hdr, sizeof(hdr), "Online (%d)##frd_online", (int)online_idx.size());
        ImGui::PushStyleColor(ImGuiCol_Header, TC(ImVec4(0.20f, 0.30f, 0.45f, 0.80f)));
        ImGui::PushStyleColor(ImGuiCol_HeaderHovered, TC(ImVec4(0.25f, 0.38f, 0.55f, 0.90f)));
        ImGui::PushStyleColor(ImGuiCol_HeaderActive, TC(ImVec4(0.30f, 0.45f, 0.65f, 1.00f)));
        bool open = ImGui::CollapsingHeader(hdr, ImGuiTreeNodeFlags_DefaultOpen);
        ImGui::PopStyleColor(3);
        if (open) {
            for (int idx : online_idx) render_friend_row(idx);
        }
    }

    ImGui::EndChild();
}

/* ── Chat window (single tabbed window with avatars) ─────────────────────── */

// Check for incoming messages from friends and auto-add to chat tabs
static void check_incoming_messages()
{
    if (!s_bridge.GetFriendCount || !s_bridge.GetFriends || !s_bridge.GetChatState) return;
    
    int friend_count = s_bridge.GetFriendCount();
    if (friend_count <= 0) return;
    
    std::vector<GSE_Friend> friends(friend_count > 256 ? 256 : friend_count);
    int count = s_bridge.GetFriends(friends.data(), (int)friends.size());
    
    for (int i = 0; i < count; ++i) {
        GSE_ChatState cs{};
        if (s_bridge.GetChatState(friends[i].steam_id, &cs) && cs.needs_attention) {
            // Friend has unread message - auto-add tab if chat window is open
            if (s_show_chat) {
                bool found = false;
                for (auto &cw : s_open_chats) {
                    if (cw.steam_id == friends[i].steam_id) { found = true; break; }
                }
                if (!found) {
                    AddonChatWindow cw{};
                    cw.steam_id = friends[i].steam_id;
                    strncpy(cw.friend_name, friends[i].name, sizeof(cw.friend_name) - 1);
                    s_open_chats.push_back(cw);
                }
            }
        }
    }
}

static void render_chat_windows()
{
    if (!s_bridge.GetChatState || !s_bridge.SendChatMessage) return;
    if (!s_show_chat) return;
    
    auto &io = ImGui::GetIO();
    
    // Window setup
    ImGui::SetNextWindowSize(ImVec2(450, 350), ImGuiCond_FirstUseEver);
    ImGui::SetNextWindowPos(ImVec2(io.DisplaySize.x * 0.5f - 225, io.DisplaySize.y * 0.5f - 175), ImGuiCond_FirstUseEver);
    ImGui::SetNextWindowBgAlpha(1.0f);  // Non-transparent window
    
    int style_colors = apply_global_style_colors();
    
    if (ImGui::Begin("Chat##gse_chat_tabbed", &s_show_chat, ImGuiWindowFlags_NoCollapse)) {
        // If no chats open, show friend picker
        if (s_open_chats.empty()) {
            ImGui::TextColored(TC(ImVec4(0.8f, 0.8f, 0.2f, 1.0f)), "Select a friend to start chatting:");
            ImGui::Spacing();
            
            // List friends to start chat with
            if (s_bridge.GetFriendCount && s_bridge.GetFriends) {
                int friend_count = s_bridge.GetFriendCount();
                if (friend_count > 0) {
                    std::vector<GSE_Friend> friends(friend_count > 64 ? 64 : friend_count);
                    int count = s_bridge.GetFriends(friends.data(), (int)friends.size());
                    
                    ImGui::BeginChild("##friend_picker", ImVec2(0, 0), true);
                    for (int i = 0; i < count; ++i) {
                        auto &f = friends[i];
                        ImGui::PushID(i);
                        
                        // Avatar + name button
                        const float avatar_size = 24.0f;
                        const IconTexture *avatar = get_or_upload_avatar(f.steam_id);
                        
                        if (avatar && avatar->valid) {
                            ImGui::Image(ImTextureRef(avatar->srv.handle), ImVec2(avatar_size, avatar_size));
                            ImGui::SameLine();
                        }
                        
                        if (ImGui::Selectable(f.name, false)) {
                            AddonChatWindow cw{};
                            cw.steam_id = f.steam_id;
                            strncpy(cw.friend_name, f.name, sizeof(cw.friend_name) - 1);
                            s_open_chats.push_back(cw);
                            s_active_chat_idx = (int)s_open_chats.size() - 1;
                            if (s_bridge.OpenChat) s_bridge.OpenChat(f.steam_id);
                        }
                        
                        ImGui::PopID();
                    }
                    ImGui::EndChild();
                } else {
                    ImGui::TextDisabled("No friends online.");
                }
            }
            
            ImGui::End();
            ImGui::PopStyleColor(style_colors);
            return;
        }
        
        // Clamp active index
        if (s_active_chat_idx < 0) s_active_chat_idx = 0;
        if (s_active_chat_idx >= (int)s_open_chats.size()) s_active_chat_idx = (int)s_open_chats.size() - 1;

        // Fetch friends list once for looking up friend info in chat headers
        std::vector<GSE_Friend> chat_friends_cache;
        if (s_bridge.GetFriendCount && s_bridge.GetFriends) {
            int fc = s_bridge.GetFriendCount();
            if (fc > 0) {
                chat_friends_cache.resize(fc > 256 ? 256 : fc);
                int cnt = s_bridge.GetFriends(chat_friends_cache.data(), (int)chat_friends_cache.size());
                chat_friends_cache.resize(cnt);
            }
        }

        // Tab bar for chats
        if (ImGui::BeginTabBar("##chat_tabs")) {
            for (size_t i = 0; i < s_open_chats.size(); ++i) {
                auto &chat = s_open_chats[i];
                
                // Check if this chat needs attention (unread)
                GSE_ChatState cs{};
                bool needs_attn = s_bridge.GetChatState(chat.steam_id, &cs) && cs.needs_attention;
                bool got_state = s_bridge.GetChatState(chat.steam_id, &cs) != 0;
                
                // Color tab if needs attention
                if (needs_attn)
                    ImGui::PushStyleColor(ImGuiCol_Tab, TC(ImVec4(0.6f, 0.4f, 0.1f, 1.0f)));
                
                bool tab_open = true;
                if (ImGui::BeginTabItem(chat.friend_name, &tab_open)) {
                    s_active_chat_idx = (int)i;

                    // Clear attention when tab is active
                    if (needs_attn && s_bridge.GetChatState)
                        cs.needs_attention = 0; // local only, bridge clears on read

                    // ---- 64px friend avatar + 3 info lines ----
                    {
                        const float avatar_size = 64.0f;
                        const IconTexture *friend_avatar = get_or_upload_avatar(chat.steam_id);
                        if (friend_avatar && friend_avatar->valid) {
                            ImGui::Image(ImTextureRef(friend_avatar->srv.handle), ImVec2(avatar_size, avatar_size));
                        } else {
                            ImVec2 p = ImGui::GetCursorScreenPos();
                            ImGui::GetWindowDrawList()->AddRectFilled(p, ImVec2(p.x + avatar_size, p.y + avatar_size), TC32(IM_COL32(60, 60, 80, 255)));
                            ImGui::Dummy(ImVec2(avatar_size, avatar_size));
                        }
                        ImGui::SameLine();

                        ImVec2 text_start = ImGui::GetCursorPos();

                        // Look up friend info for header lines
                        const GSE_Friend *finfo = nullptr;
                        for (auto &f : chat_friends_cache) {
                            if (f.steam_id == chat.steam_id) { finfo = &f; break; }
                        }

                        // Line 1: Friend name (ID: steamid)
                        ImGui::SetCursorPos(text_start);
                        ImGui::TextColored(TC(ImVec4(0.9f, 0.9f, 0.2f, 1.0f)), "%s", chat.friend_name);
                        ImGui::SameLine();
                        ImGui::TextColored(TC(ImVec4(0.6f, 0.6f, 0.6f, 1.0f)), "(ID: %llu)",
                            (unsigned long long)chat.steam_id);

                        // Line 2: Playing AppID
                        ImGui::SetCursorPosX(text_start.x);
                        if (finfo && finfo->appid != 0) {
                            const char *game = finfo->app_name[0] ? finfo->app_name : nullptr;
                            if (game)
                                ImGui::TextColored(TC(ImVec4(0.5f, 0.8f, 0.5f, 1.0f)), "Playing %s (AppID %u)", game, finfo->appid);
                            else
                                ImGui::TextColored(TC(ImVec4(0.5f, 0.8f, 0.5f, 1.0f)), "Playing AppID %u", finfo->appid);
                        } else {
                            ImGui::TextColored(TC(ImVec4(0.5f, 0.7f, 0.5f, 1.0f)), "Online");
                        }

                        // Line 3: Status
                        ImGui::SetCursorPosX(text_start.x);
                        if (finfo && finfo->in_lobby && finfo->lobby_id != 0) {
                            bool frd_is_owner = (finfo->lobby_owner_name[0] && strcmp(finfo->lobby_owner_name, finfo->name) == 0);
                            if (finfo->lobby_owner_name[0])
                                ImGui::TextColored(TC(ImVec4(0.4f, 0.8f, 0.4f, 1.0f)), "%s - %llu (%d/%d - %s)",
                                    frd_is_owner ? "Has Lobby" : "In Lobby",
                                    (unsigned long long)finfo->lobby_id, finfo->lobby_member_count, finfo->lobby_member_limit, finfo->lobby_owner_name);
                            else
                                ImGui::TextColored(TC(ImVec4(0.4f, 0.8f, 0.4f, 1.0f)), "In Lobby - %llu (%d/%d)",
                                    (unsigned long long)finfo->lobby_id, finfo->lobby_member_count, finfo->lobby_member_limit);
                        } else if (finfo && finfo->same_app) {
                            ImGui::TextColored(TC(ImVec4(0.5f, 0.7f, 0.5f, 1.0f)), "In Game");
                        } else if (finfo && finfo->appid != 0) {
                            ImGui::TextColored(TC(ImVec4(0.6f, 0.6f, 0.6f, 1.0f)), "In another game");
                        } else {
                            ImGui::TextColored(TC(ImVec4(0.5f, 0.7f, 0.5f, 1.0f)), "Online");
                        }
                    }
                    ImGui::Separator();

                    // Invite accept/refuse
                    if (got_state && cs.has_pending_invite) {
                        ImGui::TextColored(TC(ImVec4(1.0f, 0.8f, 0.2f, 1.0f)), "Pending invite from this friend!");
                        ImGui::SameLine();
                        if (ImGui::Button("Accept##accept_invite")) {
                            if (s_bridge.FriendAction)
                                s_bridge.FriendAction(chat.steam_id, GSE_FRIEND_ACTION_ACCEPT_INVITE);
                            chat.scroll_to_bottom = true;
                        }
                        ImGui::SameLine();
                        if (ImGui::Button("Refuse##refuse_invite")) {
                            if (s_bridge.FriendAction)
                                s_bridge.FriendAction(chat.steam_id, GSE_FRIEND_ACTION_REFUSE_INVITE);
                            chat.scroll_to_bottom = true;
                        }
                    }

                    // Chat history area
                    float footer_height = ImGui::GetFrameHeightWithSpacing() + 4;
                    char history_id[32];
                    snprintf(history_id, sizeof(history_id), "##chat_history_%d", (int)i);
                    ImGui::BeginChild(history_id, ImVec2(0, -footer_height), true);

                    if (got_state && cs.chat_history[0]) {
                        char *history = cs.chat_history;
                        char *line = history;
                        while (*line) {
                            char *end = strchr(line, '\n');
                            if (end) *end = '\0';

                            bool is_self = (strncmp(line, "You: ", 5) == 0);
                            bool is_invite_line = (strstr(line, "[INVITE]") != nullptr);
                            bool is_invite_accepted = (strstr(line, "[INVITE ACCEPTED]") != nullptr);
                            bool is_invite_refused = (strstr(line, "[INVITE REFUSED]") != nullptr);

                            if (is_invite_accepted)
                                ImGui::TextColored(TC(ImVec4(0.3f, 0.9f, 0.3f, 1.0f)), "%s", line);
                            else if (is_invite_refused)
                                ImGui::TextColored(TC(ImVec4(0.9f, 0.3f, 0.3f, 1.0f)), "%s", line);
                            else if (is_invite_line)
                                ImGui::TextColored(TC(ImVec4(1.0f, 0.8f, 0.2f, 1.0f)), "%s", line);
                            else if (is_self)
                                ImGui::TextColored(TC(ImVec4(0.4f, 0.8f, 1.0f, 1.0f)), "%s", line);
                            else
                                ImGui::TextColored(TC(ImVec4(0.9f, 0.9f, 0.2f, 1.0f)), "%s", line);

                            if (!end) break;
                            *end = '\n';
                            line = end + 1;
                        }
                    } else {
                        ImGui::TextDisabled("No messages yet.");
                    }

                    // Auto-scroll
                    if (chat.scroll_to_bottom || (got_state && cs.needs_attention)) {
                        ImGui::SetScrollHereY(1.0f);
                        chat.scroll_to_bottom = false;
                    }

                    ImGui::EndChild();

                    // Input bar
                    float send_btn_w = ImGui::CalcTextSize("Send").x + ImGui::GetStyle().FramePadding.x * 2 + 8;
                    ImGui::SetNextItemWidth(ImGui::GetContentRegionAvail().x - send_btn_w - 8);

                    bool send_msg = false;
                    char input_id[32];
                    snprintf(input_id, sizeof(input_id), "##chat_input_%d", (int)i);
                    if (ImGui::InputText(input_id, chat.input_buf, sizeof(chat.input_buf),
                            ImGuiInputTextFlags_EnterReturnsTrue)) {
                        send_msg = true;
                    }

                    ImGui::SameLine();
                    if (ImGui::Button("Send") || send_msg) {
                        if (chat.input_buf[0]) {
                            s_bridge.SendChatMessage(chat.steam_id, chat.input_buf);
                            chat.input_buf[0] = '\0';
                            chat.scroll_to_bottom = true;
                        }
                    }

                    ImGui::EndTabItem();
                }
                
                if (needs_attn)
                    ImGui::PopStyleColor();
                
                // Close tab
                if (!tab_open) {
                    if (s_bridge.CloseChat) s_bridge.CloseChat(chat.steam_id);
                    s_open_chats.erase(s_open_chats.begin() + i);
                    if (s_active_chat_idx >= (int)s_open_chats.size())
                        s_active_chat_idx = (int)s_open_chats.size() - 1;
                    --i; // adjust loop
                }
            }
            ImGui::EndTabBar();
        }
    }
    ImGui::End();
    ImGui::PopStyleColor(style_colors);
}

/* ── Achievement list (separate window, matching native layout exactly) ── */

static int  s_ach_sort_mode = 0;      // 0=Global %, 1=Schema Order, 2=Alphabetical
static int  s_ach_current_tab = 1;    // 0=In Progress, 1=My Achievements, 2=Groups, 3=Global Stats
static char s_ach_search_buf[256] = {};

static void render_achievement_list()
{
    if (!s_bridge.GetAchievements || !s_bridge.GetAchievementCount) return;

    int total = s_bridge.GetAchievementCount();
    if (total <= 0) return;

    int fetch = total > 2048 ? 2048 : total;
    std::vector<GSE_Achievement> achs(fetch);
    int count = s_bridge.GetAchievements(achs.data(), fetch);
    if (count <= 0) return;

    // Count achieved
    int achieved_count = 0;
    for (int i = 0; i < count; ++i)
        if (achs[i].achieved) achieved_count++;

    auto &io = ImGui::GetIO();
    float font_size = ImGui::GetFontSize();
    float noti_w = io.DisplaySize.x * NOTIF_WIDTH_FRAC;
    float min_w = font_size * 32.0f > noti_w ? font_size * 32.0f : noti_w;

    // Separate window with size constraints (matching native)
    ImGui::SetNextWindowSizeConstraints(ImVec2(min_w, font_size * 32.0f), ImVec2(8192, 8192));
    ImGui::SetNextWindowBgAlpha(1.0f);

    char ach_win_title[256];
    snprintf(ach_win_title, sizeof(ach_win_title), "%s##gse_ach", translationAchievementWindow[s_current_language]);
    if (!ImGui::Begin(ach_win_title, &s_show_achievements)) {
        ImGui::End();
        return;
    }

    // ── Completion progress bar with shadow text overlay (matching native exactly) ──
    {
        float fill = count > 0 ? (float)achieved_count / count : 0.0f;
        float pct  = fill * 100.0f;

        char left_buf[32]{};
        snprintf(left_buf, sizeof(left_buf), "%d/%d", achieved_count, count);
        char right_buf[32]{};
        snprintf(right_buf, sizeof(right_buf), "%.1f%%", pct);

        const float bar_h = font_size + ImGui::GetStyle().FramePadding.y * 2.0f;
        ImVec2 bar_pos = ImGui::GetCursorScreenPos();
        float bar_width = ImGui::GetContentRegionAvail().x;
        ImGui::ProgressBar(fill, ImVec2(-1.0f, bar_h), "");

        auto *dl = ImGui::GetWindowDrawList();
        const ImU32 shadow_col = TC32(IM_COL32(0, 0, 0, 200));
        const ImU32 text_col   = TC32(IM_COL32(255, 255, 255, 255));
        auto draw_sh = [&](ImVec2 pos, const char *text) {
            dl->AddText(ImVec2(pos.x + 1, pos.y + 1), shadow_col, text);
            dl->AddText(pos, text_col, text);
        };

        // Left: x/y
        ImVec2 left_sz = ImGui::CalcTextSize(left_buf);
        ImVec2 left_pos = { bar_pos.x + 4.0f, bar_pos.y + (bar_h - left_sz.y) * 0.5f };
        draw_sh(left_pos, left_buf);

        // Right: percentage
        ImVec2 right_sz = ImGui::CalcTextSize(right_buf);
        ImVec2 right_pos = { bar_pos.x + bar_width - right_sz.x - 4.0f, bar_pos.y + (bar_h - right_sz.y) * 0.5f };
        draw_sh(right_pos, right_buf);
    }

    // ── Reset / Simulate buttons (matching native) ──
    if (s_bridge.ResetAchievements) {
        if (ImGui::Button("Reset##ach_reset"))
            s_bridge.ResetAchievements();
        ImGui::SameLine();
    }
    if (s_bridge.SimulateAchievements) {
        if (ImGui::Button("Simulate##ach_simulate"))
            s_bridge.SimulateAchievements();
        ImGui::SameLine();
    }
    if (s_bridge.TestAchievement) {
        if (ImGui::Button(translationTestAchievement[s_current_language]))
            s_bridge.TestAchievement();
    }

    // ── Tab bar: In Progress | My Achievements | [Groups] | Global Stats ──
    bool has_groups = s_bridge.HasAchievementGroups && s_bridge.HasAchievementGroups() != 0;
    if (ImGui::BeginTabBar("##ach_tabs")) {
        if (ImGui::BeginTabItem("In Progress##ach_tab0")) {
            s_ach_current_tab = 0;
            ImGui::EndTabItem();
        }
        if (ImGui::BeginTabItem("My Achievements##ach_tab1")) {
            s_ach_current_tab = 1;
            ImGui::EndTabItem();
        }
        if (has_groups) {
            if (ImGui::BeginTabItem("Groups##ach_tab2")) {
                s_ach_current_tab = 2;
                ImGui::EndTabItem();
            }
        }
        if (ImGui::BeginTabItem("Global Stats##ach_tab3")) {
            s_ach_current_tab = 3;
            ImGui::EndTabItem();
        }
        ImGui::EndTabBar();
    }
    // safety: if groups disappeared while on Groups tab, fall back
    if (s_ach_current_tab == 2 && !has_groups) s_ach_current_tab = 1;

    // ── Search + sort controls ──
    // measure button width to size the search box dynamically
    const auto &style = ImGui::GetStyle();
    float btn_w = 0.0f;
    float spacing = style.ItemSpacing.x;
    const char *sort_labels[] = { "Sort: Global %##ach_srt", "Sort: Schema Order##ach_srt", "Sort: A-Z##ach_srt" };
    const char *srt_lbl = sort_labels[s_ach_sort_mode % 3];
    btn_w += ImGui::CalcTextSize(srt_lbl).x + style.FramePadding.x * 2.0f + spacing;
    float avail = ImGui::GetContentRegionAvail().x;
    float search_w = avail - btn_w;
    if (search_w < ImGui::GetFontSize() * 6.0f) search_w = ImGui::GetFontSize() * 6.0f;

    ImGui::SetNextItemWidth(search_w);
    ImGui::InputTextWithHint("##ach_search", "Search achievements...", s_ach_search_buf, sizeof(s_ach_search_buf));

    ImGui::SameLine();
    if (ImGui::Button(sort_labels[s_ach_sort_mode % 3]))
        s_ach_sort_mode = (s_ach_sort_mode + 1) % 3;

    ImGui::Separator();

    ImGui::BeginChild("Achievements##ach_child");

    // ── Search filter (case-insensitive substring) ──
    std::string search_lower;
    bool has_search = s_ach_search_buf[0] != '\0';
    if (has_search) {
        search_lower = s_ach_search_buf;
        std::transform(search_lower.begin(), search_lower.end(), search_lower.begin(),
            [](unsigned char c){ return (char)std::tolower(c); });
    }
    auto ach_matches_search = [&](const GSE_Achievement &a) -> bool {
        if (!has_search) return true;
        auto lower_contains = [&](const char *str) -> bool {
            std::string s = str;
            std::transform(s.begin(), s.end(), s.begin(), [](unsigned char c){ return (char)std::tolower(c); });
            return s.find(search_lower) != std::string::npos;
        };
        return lower_contains(a.title) || lower_contains(a.name) || lower_contains(a.description);
    };

    // ── Tab filter ──
    auto ach_matches_tab = [&](const GSE_Achievement &a) -> bool {
        switch (s_ach_current_tab) {
            case 0: return !a.achieved && a.max_progress > 0 && a.progress > 0;
            case 1: return true;
            case 2: return true;
            default: return true;
        }
    };

    // ── Helper: get best global % (steam → SH local → -1) ──
    auto get_ach_pct = [&](const GSE_Achievement &a) -> float {
        if (a.global_percent >= 0.0f) return a.global_percent;
        if (a.sh_local_percent >= 0.0f) return a.sh_local_percent;
        return -1.0f;
    };

    // ── Comparator (unified across all tabs) ──
    auto ach_compare = [&](int ai, int bi) -> bool {
        const auto &a = achs[ai];
        const auto &b = achs[bi];
        // hidden (locked) achievements last, except Global Stats tab
        if (s_ach_current_tab != 3) {
            bool a_hidden = a.hidden && !a.achieved;
            bool b_hidden = b.hidden && !b.achieved;
            if (a_hidden != b_hidden) return !a_hidden;
            if (a_hidden) return false;
        }
        // My Achievements / Groups: achieved first by unlock time
        if (s_ach_current_tab == 1 || s_ach_current_tab == 2) {
            if (a.achieved != b.achieved) return a.achieved > b.achieved;
            if (a.achieved) return a.unlock_time > b.unlock_time;
        }
        // sort mode
        if (s_ach_sort_mode == 0) {
            float pa = get_ach_pct(a), pb = get_ach_pct(b);
            if (pa != pb) return pa > pb;
        } else if (s_ach_sort_mode == 2) {
            int cmp = strcmp(a.title, b.title);
            if (cmp != 0) return cmp < 0;
        }
        return ai < bi;
    };

    // ── Per-achievement render lambda ──
    const float bar_h = font_size;
    const float icon_col_w = ICON_SIZE;

    auto render_ach_item = [&](int idx) {
        auto &a = achs[idx];
        bool achieved = a.achieved != 0;
        bool hidden = a.hidden && !achieved;

        ImGui::PushID(idx);
        ImGui::Separator();

        // Title
        if (hidden) ImGui::Text("[%s]", translationHiddenAchievement[s_current_language]);
        else        ImGui::Text("%s", a.title);

        // Obtainability badges
        if (a.obtainability > 0) {
            ImGui::SameLine();
            switch (a.obtainability) {
                case 1: ImGui::TextColored(TC(ImVec4(1.0f, 0.85f, 0.0f, 1.0f)), "[Missable]"); break;
                case 2: ImGui::TextColored(TC(ImVec4(0.9f, 0.2f, 0.2f, 1.0f)), "[Bugged]"); break;
                case 3: ImGui::TextColored(TC(ImVec4(1.0f, 0.6f, 0.0f, 1.0f)), "[Online Only]"); break;
                default: ImGui::TextColored(TC(ImVec4(0.7f, 0.7f, 0.7f, 1.0f)), "[Special]"); break;
            }
        }

        // Icon + description table (matching native 2-column layout)
        if (ImGui::BeginTable("##ach_tbl", 2)) {
            ImGui::TableSetupColumn("img", ImGuiTableColumnFlags_WidthFixed, icon_col_w);
            ImGui::TableSetupColumn("txt");
            ImGui::TableNextRow(ImGuiTableRowFlags_None, icon_col_w);

            ImGui::TableSetColumnIndex(0);
            {
                // Pick the appropriate icon variant: color for achieved, gray for locked
                const uint8_t *px = achieved ? a.icon_pixels : a.icon_gray_pixels;
                int pw = achieved ? a.icon_w : a.icon_gray_w;
                int ph = achieved ? a.icon_h : a.icon_gray_h;
                // Fall back to color icon if gray variant is not available
                if (!px && !achieved && a.icon_pixels) {
                    px = a.icon_pixels; pw = a.icon_w; ph = a.icon_h;
                }

                char icon_key[320];
                snprintf(icon_key, sizeof(icon_key), "ach_%s_%d", a.name, achieved ? 1 : 0);
                const IconTexture *icon = get_or_upload_icon(icon_key, px, pw, ph);
                if (icon) {
                    ImGui::Image(ImTextureRef(icon->srv.handle), ImVec2(icon_col_w, icon_col_w));
                } else {
                    // Fallback: colored placeholder
                    ImVec4 icon_col = achieved
                        ? ImVec4(0.2f, 0.7f, 0.2f, 1.0f)
                        : ImVec4(0.35f, 0.35f, 0.35f, 1.0f);
                    ImGui::PushStyleColor(ImGuiCol_Button, icon_col);
                    ImGui::PushStyleColor(ImGuiCol_ButtonHovered, icon_col);
                    ImGui::PushStyleColor(ImGuiCol_ButtonActive, icon_col);
                    ImGui::Button(achieved ? u8"\u2713" : (hidden ? "?" : u8"\u2717"),
                        ImVec2(icon_col_w, icon_col_w));
                    ImGui::PopStyleColor(3);
                }
            }

            ImGui::TableSetColumnIndex(1);
            if (!hidden) ImGui::TextWrapped("%s", a.description);
            else         ImGui::TextDisabled("%s", a.description);

            ImGui::EndTable();
        }

        // ── Status bar (ProgressBar + shadow text overlay) ──
        {
            const char *sym;
            ImU32 sym_col;
            bool has_progress = !achieved && a.max_progress > 0;
            if (achieved) {
                sym = u8"\u2713"; sym_col = TC32(COL_ACH_ACHIEVED);
            } else if (has_progress && a.progress > 0) {
                sym = u8"\u25B6"; sym_col = TC32(COL_ACH_PROGRESS);
            } else {
                sym = u8"\u2717"; sym_col = TC32(COL_ACH_LOCKED);
            }

            char date_buf[128]{};
            if (achieved && a.unlock_time > 0) {
                time_t t = (time_t)a.unlock_time;
                struct tm tm_buf;
                localtime_s(&tm_buf, &t);
                strftime(date_buf, sizeof(date_buf), "%Y/%m/%d - %H:%M:%S", &tm_buf);
            }

            char pbuf[32]{};
            bool show_progress = a.max_progress > 1 || (a.max_progress > 0 && !achieved);
            if (show_progress) snprintf(pbuf, sizeof(pbuf), "%u/%u", achieved ? a.max_progress : a.progress, a.max_progress);

            float fill = achieved ? 1.0f : (has_progress ? (float)a.progress / (float)a.max_progress : 0.0f);
            ImVec2 sbar_pos = ImGui::GetCursorScreenPos();
            float sbar_width = ImGui::GetContentRegionAvail().x;
            ImGui::ProgressBar(fill, ImVec2(-1.0f, bar_h), "");

            auto *dl = ImGui::GetWindowDrawList();
            ImFont *fnt = ImGui::GetFont();
            const float sym_font_sz = bar_h * 0.8f;
            const ImU32 shadow_col = TC32(IM_COL32(0, 0, 0, 200));

            auto draw_shadowed = [&](ImVec2 pos, ImU32 col, const char *text) {
                dl->AddText(ImVec2(pos.x + 1, pos.y + 1), shadow_col, text);
                dl->AddText(pos, col, text);
            };
            auto draw_shadowed_ex = [&](ImFont *f, float sz, ImVec2 pos, ImU32 col, const char *text) {
                dl->AddText(f, sz, ImVec2(pos.x + 1, pos.y + 1), shadow_col, text);
                dl->AddText(f, sz, pos, col, text);
            };

            // Estimate symbol size using current font metrics scaled to sym_font_sz
            ImVec2 cur_sz = ImGui::CalcTextSize(sym);
            float scale = sym_font_sz / ImGui::GetFontSize();
            ImVec2 sym_sz = { cur_sz.x * scale, cur_sz.y * scale };
            ImVec2 sym_pos = { sbar_pos.x + 4.0f, sbar_pos.y + (bar_h - sym_sz.y) * 0.5f };
            draw_shadowed_ex(fnt, sym_font_sz, sym_pos, sym_col, sym);

            if (achieved && date_buf[0]) {
                float date_x = sym_pos.x + sym_sz.x + 4.0f;
                ImVec2 date_sz = ImGui::CalcTextSize(date_buf);
                ImVec2 date_pos = { date_x, sbar_pos.y + (bar_h - date_sz.y) * 0.5f };
                draw_shadowed(date_pos, TC32(IM_COL32(255, 255, 255, 255)), date_buf);
            }

            if (show_progress && pbuf[0]) {
                ImVec2 pbar_sz = ImGui::CalcTextSize(pbuf);
                ImVec2 pbar_pos = { sbar_pos.x + (sbar_width - pbar_sz.x) * 0.5f, sbar_pos.y + (bar_h - pbar_sz.y) * 0.5f };
                draw_shadowed(pbar_pos, TC32(IM_COL32(255, 255, 255, 255)), pbuf);
            }
        }

        // ── Global % + SteamHunters community % ──
        if (a.global_percent >= 0.0f) {
            ImGui::TextDisabled(translationGlobalAchievementPercent[s_current_language], a.global_percent);
            if (a.sh_local_percent >= 0.0f) {
                ImGui::SameLine();
                ImGui::TextDisabled("| %.1f%% of hunters", a.sh_local_percent);
            }
        }

        ImGui::Separator();
        ImGui::PopID();
    };

    // ── Build filtered index list (tab + search) ──
    std::vector<int> filtered_idx;
    filtered_idx.reserve(count);
    for (int i = 0; i < count; ++i) {
        if (!ach_matches_tab(achs[i])) continue;
        if (!ach_matches_search(achs[i])) continue;
        filtered_idx.push_back(i);
    }

    // ── "My Achievements" tab: split into Unlocked/Locked sections ──
    if (s_ach_current_tab == 1) {
        std::vector<int> unlocked, locked;
        for (int fi : filtered_idx) {
            if (achs[fi].achieved) unlocked.push_back(fi);
            else locked.push_back(fi);
        }
        std::stable_sort(unlocked.begin(), unlocked.end(), ach_compare);
        std::stable_sort(locked.begin(), locked.end(), ach_compare);

        char hdr_u[64]; snprintf(hdr_u, sizeof(hdr_u), "Unlocked (%d)##ach_unlocked", (int)unlocked.size());
        char hdr_l[64]; snprintf(hdr_l, sizeof(hdr_l), "Locked (%d)##ach_locked", (int)locked.size());

        ImGui::PushStyleColor(ImGuiCol_Header, TC(ImVec4(0.15f, 0.35f, 0.15f, 0.80f)));
        ImGui::PushStyleColor(ImGuiCol_HeaderHovered, TC(ImVec4(0.20f, 0.45f, 0.20f, 0.90f)));
        ImGui::PushStyleColor(ImGuiCol_HeaderActive, TC(ImVec4(0.25f, 0.55f, 0.25f, 1.00f)));
        bool open_u = ImGui::CollapsingHeader(hdr_u, ImGuiTreeNodeFlags_DefaultOpen);
        ImGui::PopStyleColor(3);
        if (open_u) {
            for (int si : unlocked) render_ach_item(si);
        }

        ImGui::PushStyleColor(ImGuiCol_Header, TC(ImVec4(0.35f, 0.20f, 0.20f, 0.80f)));
        ImGui::PushStyleColor(ImGuiCol_HeaderHovered, TC(ImVec4(0.45f, 0.25f, 0.25f, 0.90f)));
        ImGui::PushStyleColor(ImGuiCol_HeaderActive, TC(ImVec4(0.55f, 0.30f, 0.30f, 1.00f)));
        bool open_l = ImGui::CollapsingHeader(hdr_l, ImGuiTreeNodeFlags_DefaultOpen);
        ImGui::PopStyleColor(3);
        if (open_l) {
            for (int si : locked) render_ach_item(si);
        }
    } else if (s_ach_current_tab == 2 && has_groups && s_bridge.GetAchievementGroupCount && s_bridge.GetAchievementGroups) {
        // === GROUPED RENDER (Groups tab) ===
        int group_count = s_bridge.GetAchievementGroupCount();
        std::vector<GSE_AchievementGroup> groups(group_count);
        group_count = s_bridge.GetAchievementGroups(groups.data(), group_count);

        std::unordered_set<int> filtered_set(filtered_idx.begin(), filtered_idx.end());
        std::vector<bool> rendered(count, false);
        for (int gi = 0; gi < group_count; ++gi) {
            const auto &grp = groups[gi];

            std::vector<int> grp_idx;
            for (int i = 0; i < count; ++i) {
                if (achs[i].group_index == gi && filtered_set.count(i)) {
                    grp_idx.push_back(i);
                    rendered[i] = true;
                }
            }
            if (grp_idx.empty()) continue;
            std::stable_sort(grp_idx.begin(), grp_idx.end(), ach_compare);

            std::string hdr = (grp.dlc_app_name[0] == '\0') ? "Base Game" : grp.dlc_app_name;
            if (grp.name[0] != '\0') {
                hdr += " \xe2\x80\x94 ";
                hdr += grp.name;
            }
            // count achieved in group
            int grp_done = 0;
            for (int idx : grp_idx) if (achs[idx].achieved) ++grp_done;
            char grp_count_buf[32]; snprintf(grp_count_buf, sizeof(grp_count_buf), " (%d/%d)", grp_done, (int)grp_idx.size());
            hdr += grp_count_buf;
            hdr += "##grp_";
            hdr += std::to_string(gi);

            ImGui::PushStyleColor(ImGuiCol_Header, TC(ImVec4(0.20f, 0.30f, 0.45f, 0.80f)));
            ImGui::PushStyleColor(ImGuiCol_HeaderHovered, TC(ImVec4(0.25f, 0.38f, 0.55f, 0.90f)));
            ImGui::PushStyleColor(ImGuiCol_HeaderActive, TC(ImVec4(0.30f, 0.45f, 0.65f, 1.00f)));
            bool open = ImGui::CollapsingHeader(hdr.c_str(), ImGuiTreeNodeFlags_DefaultOpen);
            ImGui::PopStyleColor(3);
            if (open) {
                for (int idx : grp_idx) render_ach_item(idx);
            }
        }

        // Ungrouped
        std::vector<int> ungrouped;
        for (int fi : filtered_idx) {
            if (!rendered[fi]) ungrouped.push_back(fi);
        }
        if (!ungrouped.empty()) {
            std::stable_sort(ungrouped.begin(), ungrouped.end(), ach_compare);
            int ug_done = 0;
            for (int idx : ungrouped) if (achs[idx].achieved) ++ug_done;
            char ug_hdr[64]; snprintf(ug_hdr, sizeof(ug_hdr), "Base Game (%d/%d)##ach_base", ug_done, (int)ungrouped.size());
            ImGui::PushStyleColor(ImGuiCol_Header, TC(ImVec4(0.20f, 0.30f, 0.45f, 0.80f)));
            ImGui::PushStyleColor(ImGuiCol_HeaderHovered, TC(ImVec4(0.25f, 0.38f, 0.55f, 0.90f)));
            ImGui::PushStyleColor(ImGuiCol_HeaderActive, TC(ImVec4(0.30f, 0.45f, 0.65f, 1.00f)));
            bool open = ImGui::CollapsingHeader(ug_hdr, ImGuiTreeNodeFlags_DefaultOpen);
            ImGui::PopStyleColor(3);
            if (open) {
                for (int idx : ungrouped) render_ach_item(idx);
            }
        }
    } else {
        // === FLAT SORTED RENDER ===
        std::stable_sort(filtered_idx.begin(), filtered_idx.end(), ach_compare);
        for (int si : filtered_idx) render_ach_item(si);
    }

    if (filtered_idx.empty()) {
        if (s_ach_current_tab == 0)
            ImGui::TextDisabled("No achievements with active progress.");
        else if (has_search)
            ImGui::TextDisabled("No achievements match your search.");
    }

    ImGui::EndChild();
    ImGui::End();
}

/* ── ReShade settings overlay tab (shown inside ReShade's overlay panel) ─ */

static void draw_settings_overlay(effect_runtime *runtime)
{
    if (!s_bridge_ok) {
        ImGui::Text("GSE bridge not connected. Is the emu loaded?");
        if (ImGui::Button("Retry"))
            try_connect_bridge();
        return;
    }

    GSE_OverlayState state{};
    s_bridge.GetState(&state);

    ImGui::Text("GSE Overlay v%u  |  App %u  |  %s", state.abi_version, state.app_id, state.username);
    ImGui::Separator();

    // Notification settings
    if (ImGui::CollapsingHeader("Notifications", ImGuiTreeNodeFlags_DefaultOpen)) {
        if (s_bridge.GetOption && s_bridge.SetOption) {
            bool ach_notif = s_bridge.GetOption(GSE_OPT_ACH_NOTIF_ENABLE) != 0;
            if (ImGui::Checkbox("Achievement notifications", &ach_notif))
                s_bridge.SetOption(GSE_OPT_ACH_NOTIF_ENABLE, ach_notif);

            bool prog_notif = s_bridge.GetOption(GSE_OPT_ACH_PROGRESS_NOTIF_ENABLE) != 0;
            if (ImGui::Checkbox("Achievement progress", &prog_notif))
                s_bridge.SetOption(GSE_OPT_ACH_PROGRESS_NOTIF_ENABLE, prog_notif);

            bool friend_notif = s_bridge.GetOption(GSE_OPT_FRIEND_NOTIF_ENABLE) != 0;
            if (ImGui::Checkbox("Friend notifications", &friend_notif))
                s_bridge.SetOption(GSE_OPT_FRIEND_NOTIF_ENABLE, friend_notif);
        }
    }

    // Warning settings
    if (ImGui::CollapsingHeader("Warnings")) {
        if (s_bridge.GetOption && s_bridge.SetOption) {
            bool dis_all = s_bridge.GetOption(GSE_OPT_DISABLE_ALL_WARNINGS) != 0;
            if (ImGui::Checkbox("Disable all warnings", &dis_all))
                s_bridge.SetOption(GSE_OPT_DISABLE_ALL_WARNINGS, dis_all);

            bool dis_bad_appid = s_bridge.GetOption(GSE_OPT_DISABLE_BAD_APPID_WARNING) != 0;
            if (ImGui::Checkbox("Disable bad AppID warning", &dis_bad_appid))
                s_bridge.SetOption(GSE_OPT_DISABLE_BAD_APPID_WARNING, dis_bad_appid);

            bool dis_local_save = s_bridge.GetOption(GSE_OPT_DISABLE_LOCAL_SAVE_WARNING) != 0;
            if (ImGui::Checkbox("Disable local save warning", &dis_local_save))
                s_bridge.SetOption(GSE_OPT_DISABLE_LOCAL_SAVE_WARNING, dis_local_save);
        }
    }

    // Display info
    if (ImGui::CollapsingHeader("Display Info")) {
        if (s_bridge.GetDisplayInfo) {
            GSE_DisplayInfo displays[8];
            int dcount = s_bridge.GetDisplayInfo(displays, 8);
            for (int d = 0; d < dcount; ++d) {
                auto &di = displays[d];
                ImGui::Text("Display %d: %s", d+1, di.name);
                ImGui::Text("  HDR: %s | BPC: %d | Gamut: %s",
                    di.hdr_enabled ? "ON" : (di.hdr_supported ? "Available" : "No"),
                    di.bpc, di.gamut);
                ImGui::Text("  TF: %s | Range: %s | Encoding: %s",
                    di.transfer, di.range, di.encoding);
            }
            if (dcount == 0)
                ImGui::TextDisabled("No display info available");
        }

        if (s_bridge.GetSDRWhiteScale) {
            float scale = s_bridge.GetSDRWhiteScale();
            ImGui::Text("SDR white scale: %.2f", scale);
        }
    }
    // Image adjustments (local overrides — runtime tweaks, not persisted)
    if (ImGui::CollapsingHeader("Image Adjustments")) {
        // Sliders update s_appearance live (visible in the label) but we only
        // invalidate the texture cache when the user *releases* the slider.
        // This avoids flushing + re-uploading every texture on every frame
        // while the user is dragging.
        static bool s_img_adj_dirty = false;

        ImGui::SliderFloat("Brightness", &s_appearance.image_brightness, 0.5f, 2.0f, "%.2f");
        if (ImGui::IsItemDeactivatedAfterEdit()) s_img_adj_dirty = true;

        ImGui::SliderFloat("Contrast",   &s_appearance.image_contrast,   0.5f, 2.0f, "%.2f");
        if (ImGui::IsItemDeactivatedAfterEdit()) s_img_adj_dirty = true;

        ImGui::SliderFloat("Gamma",      &s_appearance.image_gamma_adjust, 0.5f, 2.0f, "%.2f");
        if (ImGui::IsItemDeactivatedAfterEdit()) s_img_adj_dirty = true;

        if (ImGui::Button("Reset##img_adj")) {
            s_appearance.image_brightness   = 1.0f;
            s_appearance.image_contrast     = 1.0f;
            s_appearance.image_gamma_adjust = 1.0f;
            s_img_adj_dirty = true;
        }

        // Apply the dirty flag by forcing the cache tracking to stale values
        // so the per-frame invalidation check in on_reshade_overlay() picks it up.
        if (s_img_adj_dirty) {
            s_img_adj_dirty = false;
            s_cached_tex_brightness = -999.0f; // sentinel — guarantees adj_changed fires
        }
    }
}

/* ── Callback when ReShade overlay opens/closes (Home key) ────────────── */

static bool on_reshade_open_overlay(effect_runtime *, bool open, input_source)
{
    s_reshade_menu_open = open;
    return false;  // don't block the state change
}

/* ── DLL entry point ──────────────────────────────────────────────────── */

BOOL APIENTRY DllMain(HMODULE hModule, DWORD fdwReason, LPVOID)
{
    switch (fdwReason) {
    case DLL_PROCESS_ATTACH:
        if (!reshade::register_addon(hModule))
            return FALSE;

        // Device lifecycle for texture management
        reshade::register_event<reshade::addon_event::init_device>(on_init_device);
        reshade::register_event<reshade::addon_event::destroy_device>(on_destroy_device);
        reshade::register_event<reshade::addon_event::init_swapchain>(on_init_swapchain);
        reshade::register_event<reshade::addon_event::destroy_swapchain>(on_destroy_swapchain);

        // The reshade_overlay event fires between ImGui::NewFrame and EndFrame
        // EVERY frame, regardless of whether the ReShade overlay panel is open.
        // This is where we render notifications and the main overlay.
        reshade::register_event<reshade::addon_event::reshade_overlay>(on_reshade_overlay);

        // Detect when the ReShade overlay panel opens/closes (Home key)
        // so we can temporarily hide our overlay while it's active
        reshade::register_event<reshade::addon_event::reshade_open_overlay>(on_reshade_open_overlay);

        // Register a settings tab inside ReShade's overlay panel
        reshade::register_overlay("GSE Overlay Settings", draw_settings_overlay);

        // Try to connect to emu immediately
        try_connect_bridge();

        reshade::log::message(reshade::log::level::info, "GSE overlay addon loaded");
        break;

    case DLL_PROCESS_DETACH:
        reshade::unregister_addon(hModule);
        break;
    }

    return TRUE;
}
