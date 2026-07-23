#pragma once

#include <vector>
#include <cstdint>

#ifdef EMU_OVERLAY
#include "InGameOverlay/RendererHook.h"

namespace ScreenshotFormat {

// Converts from any ScreenshotDataFormat_t to RGBA format.
// Returns a std::vector<uint8_t> containing the RGBA image data.
// OutChannels can be 3 (RGB) or 4 (RGBA).
std::vector<uint8_t> ConvertToRGBA(const InGameOverlay::ScreenshotCallbackParameter_t* screenshot, int outChannels = 4);

}
#endif // EMU_OVERLAY
