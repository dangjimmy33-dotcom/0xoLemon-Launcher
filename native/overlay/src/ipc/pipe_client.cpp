// ============================================================
// Named Pipe Client — Implementation
// ============================================================
// Connects to \\.\pipe\007Overlay_<PID>_Cmd created by the
// launcher.  Uses overlapped I/O so reads never block.
// ============================================================

#include "pipe_client.h"

#define WIN32_LEAN_AND_MEAN
#include <Windows.h>

namespace PipeClient
{

static HANDLE s_hPipe = INVALID_HANDLE_VALUE;

bool Connect(const std::wstring& ipcName)
{
    std::wstring pipeName = L"\\\\.\\pipe\\" + ipcName + L"_Cmd";

    // Try connecting a few times — the launcher may not have
    // created the pipe yet.
    for (int attempt = 0; attempt < 10; ++attempt)
    {
        s_hPipe = CreateFileW(
            pipeName.c_str(),
            GENERIC_READ | GENERIC_WRITE,
            0, nullptr,
            OPEN_EXISTING,
            0, nullptr);

        if (s_hPipe != INVALID_HANDLE_VALUE)
        {
            // Switch to message mode so each SendCommand is one
            // discrete message.
            DWORD mode = PIPE_READMODE_MESSAGE;
            SetNamedPipeHandleState(s_hPipe, &mode, nullptr, nullptr);
            OutputDebugStringW(L"[007Overlay] Pipe connected\n");
            return true;
        }

        Sleep(200);
    }

    OutputDebugStringW(L"[007Overlay] Pipe connection failed after retries\n");
    return false;
}

void Disconnect()
{
    if (s_hPipe != INVALID_HANDLE_VALUE)
    {
        CloseHandle(s_hPipe);
        s_hPipe = INVALID_HANDLE_VALUE;
    }
}

bool SendCommand(const std::string& cmd)
{
    if (s_hPipe == INVALID_HANDLE_VALUE)
        return false;

    DWORD written = 0;
    BOOL ok = WriteFile(s_hPipe, cmd.data(),
        static_cast<DWORD>(cmd.size()), &written, nullptr);
    return ok && written == cmd.size();
}

std::string ReadCommand()
{
    if (s_hPipe == INVALID_HANDLE_VALUE)
        return {};

    // Peek first to avoid blocking.
    DWORD available = 0;
    if (!PeekNamedPipe(s_hPipe, nullptr, 0, nullptr, &available, nullptr) || available == 0)
        return {};

    char buf[512]{};
    DWORD bytesRead = 0;
    if (ReadFile(s_hPipe, buf, sizeof(buf) - 1, &bytesRead, nullptr) && bytesRead > 0)
    {
        return std::string(buf, bytesRead);
    }

    return {};
}

} // namespace PipeClient
