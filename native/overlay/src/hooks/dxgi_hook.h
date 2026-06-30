// ============================================================
// DXGI SwapChain::Present Hook
// ============================================================
#pragma once

namespace DxgiHook
{
    // Scan for the game's IDXGISwapChain and hook Present().
    // Blocks (with sleeps) until the swap chain is found.
    bool Install();

    // Unhook and restore the original Present pointer.
    void Remove();
}
