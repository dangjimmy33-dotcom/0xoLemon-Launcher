// ============================================================
// DXGI SwapChain::Present Hook — Implementation
// ============================================================
// Strategy:
//   1. Create a temporary, hidden D3D11 device + swap chain to
//      obtain the vtable address of IDXGISwapChain::Present.
//   2. Use MinHook to detour that address.
//   3. Inside the detour, call our DX11 renderer to composite
//      the overlay texture before the frame is presented.
// ============================================================

#include "dxgi_hook.h"
#include "../overlay_core.h"
#include "../renderer/dx11_renderer.h"
#include "../ipc/shared_memory.h"

#include <MinHook.h>
#include <d3d11.h>
#include <dxgi.h>

#pragma comment(lib, "d3d11.lib")
#pragma comment(lib, "dxgi.lib")

namespace DxgiHook
{

// ── Types ──────────────────────────────────────────────────
using PresentFn = HRESULT(STDMETHODCALLTYPE*)(IDXGISwapChain*, UINT, UINT);

static PresentFn  s_originalPresent = nullptr;
static bool        s_hooked          = false;
static bool        s_rendererReady   = false;

// ── Detour ─────────────────────────────────────────────────
static HRESULT STDMETHODCALLTYPE HookedPresent(
    IDXGISwapChain* pSwapChain,
    UINT            SyncInterval,
    UINT            Flags)
{
    // One-time: capture the device from the first call.
    if (!s_rendererReady)
    {
        ID3D11Device* device = nullptr;
        if (SUCCEEDED(pSwapChain->GetDevice(__uuidof(ID3D11Device), reinterpret_cast<void**>(&device))))
        {
            ID3D11DeviceContext* ctx = nullptr;
            device->GetImmediateContext(&ctx);

            Overlay::g_device    = device;
            Overlay::g_context   = ctx;
            Overlay::g_swapChain = pSwapChain;

            Dx11Renderer::Initialise(device, ctx, pSwapChain);
            s_rendererReady = true;

            OutputDebugStringW(L"[007Overlay] DX11 device captured — renderer ready\n");
        }
    }

    // Draw the overlay when active.
    if (s_rendererReady && Overlay::IsActive())
    {
        // Read the latest frame from shared memory and blit it.
        const SharedMemory::FrameInfo* frame = SharedMemory::LockRead();
        if (frame && frame->width > 0 && frame->height > 0)
        {
            Dx11Renderer::RenderFrame(frame->pixels, frame->width, frame->height);
        }
        else
        {
            // PHASE 2 TEST: If no frame from launcher, draw a semi-transparent red square (256x256)
            static uint32_t s_testBuffer[256 * 256];
            static bool s_testInit = false;
            if (!s_testInit)
            {
                for (int i = 0; i < 256 * 256; ++i)
                {
                    // RGBA format: Red=255, Green=0, Blue=0, Alpha=128
                    s_testBuffer[i] = 0x800000FF; // little-endian: R=FF, G=00, B=00, A=80
                }
                s_testInit = true;
            }
            Dx11Renderer::RenderFrame(reinterpret_cast<const uint8_t*>(s_testBuffer), 256, 256);
        }
        SharedMemory::UnlockRead();
    }

    return s_originalPresent(pSwapChain, SyncInterval, Flags);
}

// ── vtable discovery ───────────────────────────────────────
// Creates a throw-away D3D11 device to read the Present pointer
// from the swap-chain vtable.
static void* GetPresentAddress()
{
    // Create a tiny hidden window for the swap chain.
    WNDCLASSEXW wc{};
    wc.cbSize        = sizeof(wc);
    wc.lpfnWndProc   = DefWindowProcW;
    wc.hInstance      = GetModuleHandleW(nullptr);
    wc.lpszClassName  = L"007OverlayDummy";
    RegisterClassExW(&wc);

    HWND hWnd = CreateWindowExW(0, wc.lpszClassName, L"", WS_OVERLAPPED,
        0, 0, 4, 4, nullptr, nullptr, wc.hInstance, nullptr);
    if (!hWnd)
        return nullptr;

    DXGI_SWAP_CHAIN_DESC sd{};
    sd.BufferCount       = 1;
    sd.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    sd.BufferDesc.Width  = 4;
    sd.BufferDesc.Height = 4;
    sd.BufferUsage       = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    sd.OutputWindow      = hWnd;
    sd.SampleDesc.Count  = 1;
    sd.Windowed          = TRUE;
    sd.SwapEffect        = DXGI_SWAP_EFFECT_DISCARD;

    ID3D11Device*        pDev  = nullptr;
    ID3D11DeviceContext*  pCtx  = nullptr;
    IDXGISwapChain*       pSC   = nullptr;
    D3D_FEATURE_LEVEL     level = D3D_FEATURE_LEVEL_11_0;

    HRESULT hr = D3D11CreateDeviceAndSwapChain(
        nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, 0,
        &level, 1, D3D11_SDK_VERSION,
        &sd, &pSC, &pDev, nullptr, &pCtx);

    void* presentAddr = nullptr;
    if (SUCCEEDED(hr) && pSC)
    {
        // The vtable is an array of function pointers.
        // IDXGISwapChain::Present is at index 8.
        void** vtable = *reinterpret_cast<void***>(pSC);
        presentAddr = vtable[8];
    }

    if (pSC)  pSC->Release();
    if (pCtx) pCtx->Release();
    if (pDev) pDev->Release();
    DestroyWindow(hWnd);
    UnregisterClassW(wc.lpszClassName, wc.hInstance);

    return presentAddr;
}

// ── Public API ─────────────────────────────────────────────

bool Install()
{
    void* target = GetPresentAddress();
    if (!target)
    {
        OutputDebugStringW(L"[007Overlay] Failed to locate IDXGISwapChain::Present\n");
        return false;
    }

    MH_STATUS status = MH_CreateHook(target,
        reinterpret_cast<void*>(&HookedPresent),
        reinterpret_cast<void**>(&s_originalPresent));

    if (status != MH_OK)
    {
        OutputDebugStringW(L"[007Overlay] MH_CreateHook(Present) failed\n");
        return false;
    }

    status = MH_EnableHook(target);
    if (status != MH_OK)
    {
        OutputDebugStringW(L"[007Overlay] MH_EnableHook(Present) failed\n");
        return false;
    }

    s_hooked = true;
    OutputDebugStringW(L"[007Overlay] IDXGISwapChain::Present hooked\n");
    return true;
}

void Remove()
{
    if (!s_hooked)
        return;

    Dx11Renderer::Shutdown();
    MH_DisableHook(MH_ALL_HOOKS);
    s_hooked        = false;
    s_rendererReady = false;
}

} // namespace DxgiHook
