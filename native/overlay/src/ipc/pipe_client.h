// ============================================================
// Named Pipe Client — Header
// ============================================================
// Lightweight command channel between the overlay DLL (client)
// and the launcher (server).  Carries short text messages like
// "OVERLAY_SHOW", "OVERLAY_HIDE", mouse coordinates, key events.
// ============================================================
#pragma once

#include <string>

namespace PipeClient
{
    bool Connect(const std::wstring& ipcName);
    void Disconnect();

    // Send a UTF-8 command string to the launcher.
    bool SendCommand(const std::string& cmd);

    // Non-blocking read.  Returns empty string if nothing available.
    std::string ReadCommand();
}
