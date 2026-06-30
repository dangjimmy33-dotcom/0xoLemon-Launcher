// ============================================================
// Input Hook — Header
// ============================================================
// Subclasses the game window's WndProc so that when the overlay
// is active, mouse/keyboard messages are consumed by the overlay
// instead of being forwarded to the game.
// ============================================================
#pragma once

namespace InputHook
{
    // Find the game's main window and subclass its WndProc.
    void Install();

    // Restore the original WndProc.
    void Remove();
}
