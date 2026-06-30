#include <Windows.h>
#include <d3d11.h>
#pragma comment(lib, "d3d11.lib")
#pragma comment(lib, "user32.lib")

LRESULT CALLBACK WndProc(HWND hWnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    if (msg == WM_DESTROY) {
        PostQuitMessage(0);
        return 0;
    }
    return DefWindowProc(hWnd, msg, wParam, lParam);
}

int WINAPI WinMain(HINSTANCE hInstance, HINSTANCE, LPSTR, int) {
    WNDCLASSEX wc = { sizeof(WNDCLASSEX), CS_CLASSDC, WndProc, 0, 0, hInstance, NULL, NULL, NULL, NULL, L"TestDX11", NULL };
    RegisterClassEx(&wc);
    HWND hWnd = CreateWindow(wc.lpszClassName, L"Dummy DX11 Game (Test Overlay)", WS_OVERLAPPEDWINDOW, 100, 100, 800, 600, NULL, NULL, wc.hInstance, NULL);

    DXGI_SWAP_CHAIN_DESC sd = {};
    sd.BufferCount = 1;
    sd.BufferDesc.Width = 800;
    sd.BufferDesc.Height = 600;
    sd.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    sd.BufferDesc.RefreshRate.Numerator = 60;
    sd.BufferDesc.RefreshRate.Denominator = 1;
    sd.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    sd.OutputWindow = hWnd;
    sd.SampleDesc.Count = 1;
    sd.SampleDesc.Quality = 0;
    sd.Windowed = TRUE;

    ID3D11Device* device = nullptr;
    ID3D11DeviceContext* ctx = nullptr;
    IDXGISwapChain* swapchain = nullptr;

    D3D11CreateDeviceAndSwapChain(NULL, D3D_DRIVER_TYPE_HARDWARE, NULL, 0, NULL, 0, D3D11_SDK_VERSION, &sd, &swapchain, &device, NULL, &ctx);

    ID3D11Texture2D* backBuffer = nullptr;
    swapchain->GetBuffer(0, __uuidof(ID3D11Texture2D), (LPVOID*)&backBuffer);
    ID3D11RenderTargetView* rtv = nullptr;
    device->CreateRenderTargetView(backBuffer, NULL, &rtv);
    backBuffer->Release();

    ShowWindow(hWnd, SW_SHOWDEFAULT);
    UpdateWindow(hWnd);

    MSG msg = {};
    while (msg.message != WM_QUIT) {
        if (PeekMessage(&msg, NULL, 0, 0, PM_REMOVE)) {
            TranslateMessage(&msg);
            DispatchMessage(&msg);
        } else {
            float color[4] = { 0.0f, 0.2f, 0.4f, 1.0f }; // Blue-ish background
            ctx->OMSetRenderTargets(1, &rtv, NULL);
            ctx->ClearRenderTargetView(rtv, color);
            swapchain->Present(1, 0);
        }
    }

    rtv->Release();
    swapchain->Release();
    ctx->Release();
    device->Release();
    DestroyWindow(hWnd);
    UnregisterClass(wc.lpszClassName, wc.hInstance);
    return 0;
}
