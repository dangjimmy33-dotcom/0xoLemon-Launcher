// ============================================================
// DX11 Overlay Renderer — Implementation
// ============================================================
// Draws a fullscreen-quad textured with the RGBA pixel data
// received from the launcher via shared memory.
//
// Pipeline:
//   1. Vertex shader outputs a full-screen triangle (no VB).
//   2. Pixel shader samples the overlay texture with alpha.
//   3. Alpha blending composites over the game's back buffer.
// ============================================================

#include "dx11_renderer.h"
#include <d3dcompiler.h>
#include <wrl/client.h>

#pragma comment(lib, "d3dcompiler.lib")

using Microsoft::WRL::ComPtr;

namespace Dx11Renderer
{

// ── Shader source (HLSL) ───────────────────────────────────
static const char* s_shaderSrc = R"(
// Fullscreen triangle — no vertex buffer needed.
struct VS_OUT {
    float4 pos : SV_Position;
    float2 uv  : TEXCOORD0;
};

VS_OUT VSMain(uint id : SV_VertexID)
{
    VS_OUT o;
    // Generate a triangle that covers [-1,1] clip space.
    o.uv  = float2((id << 1) & 2, id & 2);
    o.pos = float4(o.uv * float2(2, -2) + float2(-1, 1), 0, 1);
    return o;
}

Texture2D    overlayTex : register(t0);
SamplerState linearSamp : register(s0);

float4 PSMain(VS_OUT inp) : SV_Target
{
    return overlayTex.Sample(linearSamp, inp.uv);
}
)";

// ── GPU resources ──────────────────────────────────────────
static ID3D11Device*         s_device  = nullptr;
static ID3D11DeviceContext*  s_ctx     = nullptr;
static IDXGISwapChain*       s_sc      = nullptr;

static ComPtr<ID3D11VertexShader>      s_vs;
static ComPtr<ID3D11PixelShader>       s_ps;
static ComPtr<ID3D11BlendState>        s_blendState;
static ComPtr<ID3D11SamplerState>      s_sampler;
static ComPtr<ID3D11Texture2D>         s_texture;
static ComPtr<ID3D11ShaderResourceView> s_srv;
static ComPtr<ID3D11RasterizerState>   s_rasterState;

static uint32_t s_texWidth  = 0;
static uint32_t s_texHeight = 0;
static bool     s_ready     = false;

// ── helpers ────────────────────────────────────────────────
static bool CompileShaders()
{
    ComPtr<ID3DBlob> vsBlob, psBlob, errBlob;

    HRESULT hr = D3DCompile(s_shaderSrc, strlen(s_shaderSrc), "overlay",
        nullptr, nullptr, "VSMain", "vs_5_0", 0, 0, &vsBlob, &errBlob);
    if (FAILED(hr))
    {
        OutputDebugStringA(errBlob ? (char*)errBlob->GetBufferPointer() : "VS compile failed");
        return false;
    }

    hr = D3DCompile(s_shaderSrc, strlen(s_shaderSrc), "overlay",
        nullptr, nullptr, "PSMain", "ps_5_0", 0, 0, &psBlob, &errBlob);
    if (FAILED(hr))
    {
        OutputDebugStringA(errBlob ? (char*)errBlob->GetBufferPointer() : "PS compile failed");
        return false;
    }

    hr = s_device->CreateVertexShader(vsBlob->GetBufferPointer(), vsBlob->GetBufferSize(), nullptr, &s_vs);
    if (FAILED(hr)) return false;

    hr = s_device->CreatePixelShader(psBlob->GetBufferPointer(), psBlob->GetBufferSize(), nullptr, &s_ps);
    if (FAILED(hr)) return false;

    return true;
}

static bool CreatePipelineState()
{
    // Alpha blending: src.a * src + (1-src.a) * dst
    D3D11_BLEND_DESC bd{};
    bd.RenderTarget[0].BlendEnable           = TRUE;
    bd.RenderTarget[0].SrcBlend              = D3D11_BLEND_SRC_ALPHA;
    bd.RenderTarget[0].DestBlend             = D3D11_BLEND_INV_SRC_ALPHA;
    bd.RenderTarget[0].BlendOp               = D3D11_BLEND_OP_ADD;
    bd.RenderTarget[0].SrcBlendAlpha         = D3D11_BLEND_ONE;
    bd.RenderTarget[0].DestBlendAlpha        = D3D11_BLEND_INV_SRC_ALPHA;
    bd.RenderTarget[0].BlendOpAlpha          = D3D11_BLEND_OP_ADD;
    bd.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
    if (FAILED(s_device->CreateBlendState(&bd, &s_blendState)))
        return false;

    // Linear sampler
    D3D11_SAMPLER_DESC sd{};
    sd.Filter   = D3D11_FILTER_MIN_MAG_MIP_LINEAR;
    sd.AddressU = D3D11_TEXTURE_ADDRESS_CLAMP;
    sd.AddressV = D3D11_TEXTURE_ADDRESS_CLAMP;
    sd.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
    if (FAILED(s_device->CreateSamplerState(&sd, &s_sampler)))
        return false;

    // No-cull rasterizer (we draw a single triangle)
    D3D11_RASTERIZER_DESC rd{};
    rd.FillMode = D3D11_FILL_SOLID;
    rd.CullMode = D3D11_CULL_NONE;
    if (FAILED(s_device->CreateRasterizerState(&rd, &s_rasterState)))
        return false;

    return true;
}

static bool EnsureTexture(uint32_t w, uint32_t h)
{
    if (s_texture && s_texWidth == w && s_texHeight == h)
        return true;

    s_texture.Reset();
    s_srv.Reset();

    D3D11_TEXTURE2D_DESC td{};
    td.Width            = w;
    td.Height           = h;
    td.MipLevels        = 1;
    td.ArraySize        = 1;
    td.Format           = DXGI_FORMAT_R8G8B8A8_UNORM;
    td.SampleDesc.Count = 1;
    td.Usage            = D3D11_USAGE_DYNAMIC;
    td.BindFlags        = D3D11_BIND_SHADER_RESOURCE;
    td.CPUAccessFlags   = D3D11_CPU_ACCESS_WRITE;

    if (FAILED(s_device->CreateTexture2D(&td, nullptr, &s_texture)))
        return false;

    D3D11_SHADER_RESOURCE_VIEW_DESC srvd{};
    srvd.Format                    = td.Format;
    srvd.ViewDimension             = D3D11_SRV_DIMENSION_TEXTURE2D;
    srvd.Texture2D.MipLevels       = 1;
    if (FAILED(s_device->CreateShaderResourceView(s_texture.Get(), &srvd, &s_srv)))
        return false;

    s_texWidth  = w;
    s_texHeight = h;
    return true;
}

// ── Public API ─────────────────────────────────────────────

void Initialise(ID3D11Device* device, ID3D11DeviceContext* ctx, IDXGISwapChain* sc)
{
    s_device = device;
    s_ctx    = ctx;
    s_sc     = sc;

    if (!CompileShaders())
    {
        OutputDebugStringW(L"[007Overlay] Shader compilation failed\n");
        return;
    }

    if (!CreatePipelineState())
    {
        OutputDebugStringW(L"[007Overlay] Pipeline state creation failed\n");
        return;
    }

    s_ready = true;
    OutputDebugStringW(L"[007Overlay] DX11 renderer initialised\n");
}

void RenderFrame(const uint8_t* pixels, uint32_t width, uint32_t height)
{
    if (!s_ready || !pixels || width == 0 || height == 0)
        return;

    if (!EnsureTexture(width, height))
        return;

    // Upload pixel data to the dynamic texture.
    D3D11_MAPPED_SUBRESOURCE mapped{};
    if (SUCCEEDED(s_ctx->Map(s_texture.Get(), 0, D3D11_MAP_WRITE_DISCARD, 0, &mapped)))
    {
        const uint32_t srcPitch = width * 4;
        for (uint32_t y = 0; y < height; ++y)
        {
            memcpy(
                static_cast<uint8_t*>(mapped.pData) + y * mapped.RowPitch,
                pixels + y * srcPitch,
                srcPitch);
        }
        s_ctx->Unmap(s_texture.Get(), 0);
    }

    // Save current state so we can restore it after drawing.
    ComPtr<ID3D11RenderTargetView> prevRTV;
    ComPtr<ID3D11DepthStencilView> prevDSV;
    s_ctx->OMGetRenderTargets(1, &prevRTV, &prevDSV);

    // Get the back buffer and create a temporary RTV.
    ComPtr<ID3D11Texture2D> backBuffer;
    s_sc->GetBuffer(0, __uuidof(ID3D11Texture2D), reinterpret_cast<void**>(backBuffer.GetAddressOf()));
    if (!backBuffer) return;

    ComPtr<ID3D11RenderTargetView> rtv;
    s_device->CreateRenderTargetView(backBuffer.Get(), nullptr, &rtv);
    if (!rtv) return;

    // Set pipeline.
    s_ctx->OMSetRenderTargets(1, rtv.GetAddressOf(), nullptr);
    float blendFactor[4] = { 0, 0, 0, 0 };
    s_ctx->OMSetBlendState(s_blendState.Get(), blendFactor, 0xFFFFFFFF);
    s_ctx->RSSetState(s_rasterState.Get());
    s_ctx->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    s_ctx->IASetInputLayout(nullptr);
    s_ctx->VSSetShader(s_vs.Get(), nullptr, 0);
    s_ctx->PSSetShader(s_ps.Get(), nullptr, 0);
    s_ctx->PSSetShaderResources(0, 1, s_srv.GetAddressOf());
    s_ctx->PSSetSamplers(0, 1, s_sampler.GetAddressOf());

    // Viewport — match the back buffer.
    D3D11_TEXTURE2D_DESC bbDesc{};
    backBuffer->GetDesc(&bbDesc);
    D3D11_VIEWPORT vp{ 0, 0, (float)bbDesc.Width, (float)bbDesc.Height, 0, 1 };
    s_ctx->RSSetViewports(1, &vp);

    // Draw fullscreen triangle (3 vertices, no VB).
    s_ctx->Draw(3, 0);

    // Restore previous render target.
    ID3D11RenderTargetView* prevRTVRaw = prevRTV.Get();
    s_ctx->OMSetRenderTargets(1, &prevRTVRaw, prevDSV.Get());
}

void Shutdown()
{
    s_vs.Reset();
    s_ps.Reset();
    s_blendState.Reset();
    s_sampler.Reset();
    s_texture.Reset();
    s_srv.Reset();
    s_rasterState.Reset();
    s_ready = false;
}

} // namespace Dx11Renderer
