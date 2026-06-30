// ============================================================
// Shared Memory — Header
// ============================================================
// The launcher maps a shared-memory region and writes RGBA
// frames into it.  The overlay DLL reads them.
// ============================================================
#pragma once

#include <cstdint>
#include <string>

namespace SharedMemory
{
    // ── Frame header written by the launcher ───────────────
    struct FrameInfo
    {
        uint32_t width;        // pixels
        uint32_t height;       // pixels
        uint32_t stride;       // bytes per row (usually width * 4)
        uint32_t frameSeq;     // monotonic counter — lets the reader
                               // skip unchanged frames
        uint8_t  pixels[1];    // RGBA data follows (variable length)
    };

    // Open (or create) the shared-memory object.
    bool Open(const std::wstring& name);

    // Lock the region for reading and return a pointer to the
    // current frame.  Returns nullptr if no frame is available.
    const FrameInfo* LockRead();
    void             UnlockRead();

    // Close the mapping.
    void Close();
}
