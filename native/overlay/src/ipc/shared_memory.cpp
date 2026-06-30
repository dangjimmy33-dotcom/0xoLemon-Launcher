// ============================================================
// Shared Memory — Implementation
// ============================================================
// Uses Windows file-mapping objects.  The launcher creates the
// mapping and writes frames; this DLL opens and reads them.
// A lightweight Mutex serialises access.
// ============================================================

#include "shared_memory.h"

#define WIN32_LEAN_AND_MEAN
#include <Windows.h>
#include <string>

namespace SharedMemory
{

// Maximum overlay resolution: 1920×1080 RGBA = ~8 MB + header.
static constexpr DWORD  kMaxSize = sizeof(FrameInfo) + 1920 * 1080 * 4;

static HANDLE  s_hMapping = nullptr;
static HANDLE  s_hMutex   = nullptr;
static void*   s_pView    = nullptr;
static bool    s_locked   = false;

bool Open(const std::wstring& name)
{
    // Try to open an existing mapping created by the launcher.
    std::wstring mapName   = name + L"_SharedMem";
    std::wstring mutexName = name + L"_Mutex";

    s_hMapping = OpenFileMappingW(FILE_MAP_READ, FALSE, mapName.c_str());
    if (!s_hMapping)
    {
        // Launcher hasn't created it yet — create our own so
        // we're ready when it writes the first frame.
        s_hMapping = CreateFileMappingW(
            INVALID_HANDLE_VALUE, nullptr, PAGE_READWRITE,
            0, kMaxSize, mapName.c_str());
        if (!s_hMapping)
            return false;
    }

    s_pView = MapViewOfFile(s_hMapping, FILE_MAP_READ, 0, 0, 0);
    if (!s_pView)
    {
        CloseHandle(s_hMapping);
        s_hMapping = nullptr;
        return false;
    }

    s_hMutex = CreateMutexW(nullptr, FALSE, mutexName.c_str());
    return true;
}

const FrameInfo* LockRead()
{
    if (!s_pView || !s_hMutex)
        return nullptr;

    DWORD result = WaitForSingleObject(s_hMutex, 2); // 2 ms timeout
    if (result != WAIT_OBJECT_0 && result != WAIT_ABANDONED)
        return nullptr;

    s_locked = true;
    return reinterpret_cast<const FrameInfo*>(s_pView);
}

void UnlockRead()
{
    if (s_locked && s_hMutex)
    {
        ReleaseMutex(s_hMutex);
        s_locked = false;
    }
}

void Close()
{
    if (s_locked)
        UnlockRead();

    if (s_pView)
    {
        UnmapViewOfFile(s_pView);
        s_pView = nullptr;
    }
    if (s_hMapping)
    {
        CloseHandle(s_hMapping);
        s_hMapping = nullptr;
    }
    if (s_hMutex)
    {
        CloseHandle(s_hMutex);
        s_hMutex = nullptr;
    }
}

} // namespace SharedMemory
