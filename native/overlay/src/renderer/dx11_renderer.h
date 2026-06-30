// ============================================================
// DX11 Overlay Renderer
// ============================================================
#pragma once

#include <d3d11.h>
#include <cstdint>

namespace Dx11Renderer
{
    // Call once after capturing the game's device.
    void Initialise(ID3D11Device* device, ID3D11DeviceContext* ctx, IDXGISwapChain* sc);

    // Render an RGBA pixel buffer as a fullscreen-quad overlay.
    void RenderFrame(const uint8_t* pixels, uint32_t width, uint32_t height);

    // Release all GPU resources.
    void Shutdown();
}
