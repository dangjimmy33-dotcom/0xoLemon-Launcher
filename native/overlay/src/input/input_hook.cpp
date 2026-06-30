// ============================================================
// Input Hook — Implementation
// ============================================================
// Uses SetWindowLongPtr(GWLP_WNDPROC) to intercept messages.
// When the overlay is active:
//   • Mouse & keyboard messages are consumed (not forwarded).
//   • They are serialised and sent to the launcher via the
//     named pipe so the WebView can react to them.
// The hotkey Shift+Tab toggles the overlay on/off.
// ============================================================

#include "input_hook.h"
#include "../overlay_core.h"
#include "../ipc/pipe_client.h"

#define WIN32_LEAN_AND_MEAN
#include <Windows.h>
#include <string>
#include <sstream>

namespace InputHook
{

static HWND    s_gameWindow   = nullptr;
static WNDPROC s_originalProc = nullptr;

// ── Find the game window ───────────────────────────────────
// Enumerate top-level windows belonging to this process.
static BOOL CALLBACK EnumWindowsCallback(HWND hWnd, LPARAM lParam)
{
    DWORD pid = 0;
    GetWindowThreadProcessId(hWnd, &pid);
    if (pid == GetCurrentProcessId() && IsWindowVisible(hWnd))
    {
        // Pick the first visible window owned by our process.
        *reinterpret_cast<HWND*>(lParam) = hWnd;
        return FALSE; // stop enumerating
    }
    return TRUE;
}

static HWND FindGameWindow()
{
    HWND hWnd = nullptr;

    // The game may take a moment to create its window, so we
    // retry a few times with a short sleep.
    for (int i = 0; i < 50; ++i)   // up to ~5 seconds
    {
        EnumWindows(EnumWindowsCallback, reinterpret_cast<LPARAM>(&hWnd));
        if (hWnd)
            return hWnd;
        Sleep(100);
    }

    return nullptr;
}

// ── Serialize input event for IPC ──────────────────────────
static void ForwardMouseEvent(const char* type, int x, int y)
{
    std::ostringstream ss;
    ss << "INPUT_MOUSE " << type << " " << x << " " << y;
    PipeClient::SendCommand(ss.str());
}

static void ForwardKeyEvent(const char* type, WPARAM vk)
{
    std::ostringstream ss;
    ss << "INPUT_KEY " << type << " " << vk;
    PipeClient::SendCommand(ss.str());
}

// ── Hooked WndProc ─────────────────────────────────────────
static LRESULT CALLBACK OverlayWndProc(HWND hWnd, UINT msg, WPARAM wParam, LPARAM lParam)
{
    // Shift+F1 toggles the overlay regardless of state.
    if (msg == WM_KEYDOWN && wParam == VK_F1)
    {
        if (GetAsyncKeyState(VK_SHIFT) & 0x8000)
        {
            Overlay::Toggle();
            return 0; // consume the key
        }
    }

    // When the overlay is NOT active, pass everything through.
    if (!Overlay::IsActive())
        return CallWindowProcW(s_originalProc, hWnd, msg, wParam, lParam);

    // ── Overlay is active — intercept input ────────────────
    switch (msg)
    {
    // Mouse movement
    case WM_MOUSEMOVE:
        ForwardMouseEvent("MOVE", LOWORD(lParam), HIWORD(lParam));
        return 0;

    case WM_LBUTTONDOWN:
        ForwardMouseEvent("LDOWN", LOWORD(lParam), HIWORD(lParam));
        return 0;
    case WM_LBUTTONUP:
        ForwardMouseEvent("LUP", LOWORD(lParam), HIWORD(lParam));
        return 0;

    case WM_RBUTTONDOWN:
        ForwardMouseEvent("RDOWN", LOWORD(lParam), HIWORD(lParam));
        return 0;
    case WM_RBUTTONUP:
        ForwardMouseEvent("RUP", LOWORD(lParam), HIWORD(lParam));
        return 0;

    case WM_MOUSEWHEEL:
        ForwardMouseEvent("WHEEL", GET_WHEEL_DELTA_WPARAM(wParam), 0);
        return 0;

    // Keyboard
    case WM_KEYDOWN:
    case WM_SYSKEYDOWN:
        ForwardKeyEvent("DOWN", wParam);
        return 0;
    case WM_KEYUP:
    case WM_SYSKEYUP:
        ForwardKeyEvent("UP", wParam);
        return 0;
    case WM_CHAR:
        {
            std::ostringstream ss;
            ss << "INPUT_CHAR " << static_cast<unsigned>(wParam);
            PipeClient::SendCommand(ss.str());
        }
        return 0;

    // Cursor — show the system cursor when overlay is active.
    case WM_SETCURSOR:
        SetCursor(LoadCursorW(nullptr, (LPCWSTR)IDC_ARROW));
        return TRUE;

    default:
        break;
    }

    // Everything else (resize, paint, etc.) goes to the game.
    return CallWindowProcW(s_originalProc, hWnd, msg, wParam, lParam);
}

// ── Public API ─────────────────────────────────────────────

void Install()
{
    s_gameWindow = FindGameWindow();
    if (!s_gameWindow)
    {
        OutputDebugStringW(L"[007Overlay] Could not find game window\n");
        return;
    }

    s_originalProc = reinterpret_cast<WNDPROC>(
        SetWindowLongPtrW(s_gameWindow, GWLP_WNDPROC,
            reinterpret_cast<LONG_PTR>(OverlayWndProc)));

    if (s_originalProc)
        OutputDebugStringW(L"[007Overlay] WndProc hooked\n");
    else
        OutputDebugStringW(L"[007Overlay] WndProc hook failed\n");
}

void Remove()
{
    if (s_gameWindow && s_originalProc)
    {
        SetWindowLongPtrW(s_gameWindow, GWLP_WNDPROC,
            reinterpret_cast<LONG_PTR>(s_originalProc));
        s_originalProc = nullptr;
        OutputDebugStringW(L"[007Overlay] WndProc restored\n");
    }
}

} // namespace InputHook
