#include "dll/screenshot_format.h"
#include <algorithm>
#include <cmath>

#ifdef EMU_OVERLAY

namespace ScreenshotFormat {

// Simple float helper to uint8
inline uint8_t FloatToByte(float val) {
    if (val < 0.0f) val = 0.0f;
    if (val > 1.0f) val = 1.0f;
    return static_cast<uint8_t>(val * 255.0f);
}

inline uint8_t Unorm16ToByte(uint16_t val) {
    return static_cast<uint8_t>(val >> 8);
}

std::vector<uint8_t> ConvertToRGBA(const InGameOverlay::ScreenshotCallbackParameter_t* screenshot, int outChannels) {
    std::vector<uint8_t> outData;
    if (!screenshot || !screenshot->Data || screenshot->Width == 0 || screenshot->Height == 0) {
        return outData;
    }

    uint32_t width = screenshot->Width;
    uint32_t height = screenshot->Height;
    outData.resize(width * height * outChannels);

    uint8_t* srcRow = static_cast<uint8_t*>(screenshot->Data);
    uint8_t* dst = outData.data();

    for (uint32_t y = 0; y < height; ++y) {
        uint8_t* srcPixel = srcRow;
        for (uint32_t x = 0; x < width; ++x) {
            uint8_t r = 0, g = 0, b = 0, a = 255;

            switch (screenshot->Format) {
                case InGameOverlay::ScreenshotDataFormat_t::R8G8B8:
                    r = srcPixel[0];
                    g = srcPixel[1];
                    b = srcPixel[2];
                    break;
                case InGameOverlay::ScreenshotDataFormat_t::R8G8B8A8:
                    r = srcPixel[0];
                    g = srcPixel[1];
                    b = srcPixel[2];
                    a = srcPixel[3];
                    break;
                case InGameOverlay::ScreenshotDataFormat_t::B8G8R8A8:
                    b = srcPixel[0];
                    g = srcPixel[1];
                    r = srcPixel[2];
                    a = srcPixel[3];
                    break;
                case InGameOverlay::ScreenshotDataFormat_t::B8G8R8X8:
                    b = srcPixel[0];
                    g = srcPixel[1];
                    r = srcPixel[2];
                    a = 255;
                    break;
                case InGameOverlay::ScreenshotDataFormat_t::A8R8G8B8:
                case InGameOverlay::ScreenshotDataFormat_t::X8R8G8B8:
                    // D3D/little-endian format: ARGB DWORD -> memory order BGRA
                    b = srcPixel[0];
                    g = srcPixel[1];
                    r = srcPixel[2];
                    a = (screenshot->Format == InGameOverlay::ScreenshotDataFormat_t::A8R8G8B8) ? srcPixel[3] : 255;
                    break;

                case InGameOverlay::ScreenshotDataFormat_t::A2R10G10B10: {
                    uint32_t pixel = *reinterpret_cast<uint32_t*>(srcPixel);
                    // 10 bits per channel in ARGB layout
                    b = static_cast<uint8_t>((pixel & 0x3FF) >> 2);
                    g = static_cast<uint8_t>(((pixel >> 10) & 0x3FF) >> 2);
                    r = static_cast<uint8_t>(((pixel >> 20) & 0x3FF) >> 2);
                    a = static_cast<uint8_t>((pixel >> 30) * 85); // 2 bits to 8 bits (0-3 -> 0-255)
                    break;
                }
                case InGameOverlay::ScreenshotDataFormat_t::A2B10G10R10: {
                    uint32_t pixel = *reinterpret_cast<uint32_t*>(srcPixel);
                    r = static_cast<uint8_t>((pixel & 0x3FF) >> 2);
                    g = static_cast<uint8_t>(((pixel >> 10) & 0x3FF) >> 2);
                    b = static_cast<uint8_t>(((pixel >> 20) & 0x3FF) >> 2);
                    a = static_cast<uint8_t>((pixel >> 30) * 85);
                    break;
                }
                case InGameOverlay::ScreenshotDataFormat_t::R10G10B10A2: {
                    uint32_t pixel = *reinterpret_cast<uint32_t*>(srcPixel);
                    r = static_cast<uint8_t>((pixel & 0x3FF) >> 2);
                    g = static_cast<uint8_t>(((pixel >> 10) & 0x3FF) >> 2);
                    b = static_cast<uint8_t>(((pixel >> 20) & 0x3FF) >> 2);
                    a = static_cast<uint8_t>((pixel >> 30) * 85);
                    break;
                }

                case InGameOverlay::ScreenshotDataFormat_t::R5G6B5: {
                    uint16_t pixel = *reinterpret_cast<uint16_t*>(srcPixel);
                    r = ((pixel >> 11) & 0x1F) << 3;
                    g = ((pixel >> 5) & 0x3F) << 2;
                    b = (pixel & 0x1F) << 3;
                    break;
                }
                case InGameOverlay::ScreenshotDataFormat_t::B5G6R5: {
                    uint16_t pixel = *reinterpret_cast<uint16_t*>(srcPixel);
                    b = ((pixel >> 11) & 0x1F) << 3;
                    g = ((pixel >> 5) & 0x3F) << 2;
                    r = (pixel & 0x1F) << 3;
                    break;
                }
                case InGameOverlay::ScreenshotDataFormat_t::A1R5G5B5:
                case InGameOverlay::ScreenshotDataFormat_t::X1R5G5B5: {
                    uint16_t pixel = *reinterpret_cast<uint16_t*>(srcPixel);
                    b = (pixel & 0x1F) << 3;
                    g = ((pixel >> 5) & 0x1F) << 3;
                    r = ((pixel >> 10) & 0x1F) << 3;
                    a = (screenshot->Format == InGameOverlay::ScreenshotDataFormat_t::A1R5G5B5 && (pixel >> 15)) ? 255 : 0;
                    break;
                }
                case InGameOverlay::ScreenshotDataFormat_t::B5G5R5A1: {
                    uint16_t pixel = *reinterpret_cast<uint16_t*>(srcPixel);
                    a = (pixel & 0x01) ? 255 : 0;
                    r = ((pixel >> 1) & 0x1F) << 3;
                    g = ((pixel >> 6) & 0x1F) << 3;
                    b = ((pixel >> 11) & 0x1F) << 3;
                    break;
                }

                case InGameOverlay::ScreenshotDataFormat_t::R16G16B16A16_FLOAT: {
                    // Half float (16-bit float) format. We can rough-cast half to float.
                    // To keep it simple, we can convert half-precision float bits to single-precision float.
                    auto halfToFloat = [](uint16_t h) -> float {
                        uint32_t sign = (h & 0x8000) << 16;
                        uint32_t exp = (h & 0x7C00) >> 10;
                        uint32_t mant = h & 0x03FF;
                        if (exp == 0) {
                            if (mant == 0) return 0.0f;
                            while ((mant & 0x0400) == 0) {
                                mant <<= 1;
                                exp--;
                            }
                            exp++;
                            mant &= ~0x0400;
                        } else if (exp == 31) {
                            exp = 255;
                        } else {
                            exp = exp - 15 + 127;
                        }
                        uint32_t f = sign | (exp << 23) | (mant << 13);
                        return *reinterpret_cast<float*>(&f);
                    };
                    uint16_t* hf = reinterpret_cast<uint16_t*>(srcPixel);
                    r = FloatToByte(halfToFloat(hf[0]));
                    g = FloatToByte(halfToFloat(hf[1]));
                    b = FloatToByte(halfToFloat(hf[2]));
                    a = FloatToByte(halfToFloat(hf[3]));
                    break;
                }
                case InGameOverlay::ScreenshotDataFormat_t::R16G16B16A16_UNORM: {
                    uint16_t* un = reinterpret_cast<uint16_t*>(srcPixel);
                    r = Unorm16ToByte(un[0]);
                    g = Unorm16ToByte(un[1]);
                    b = Unorm16ToByte(un[2]);
                    a = Unorm16ToByte(un[3]);
                    break;
                }
                case InGameOverlay::ScreenshotDataFormat_t::R32G32B32A32_FLOAT: {
                    float* fl = reinterpret_cast<float*>(srcPixel);
                    r = FloatToByte(fl[0]);
                    g = FloatToByte(fl[1]);
                    b = FloatToByte(fl[2]);
                    a = FloatToByte(fl[3]);
                    break;
                }

                default:
                    // Unknown or unsupported format: default to standard RGBA (4 bytes)
                    r = srcPixel[0];
                    g = srcPixel[1];
                    b = srcPixel[2];
                    a = srcPixel[3];
                    break;
            }

            dst[0] = r;
            dst[1] = g;
            dst[2] = b;
            if (outChannels == 4) {
                dst[3] = 255;
            }
            dst += outChannels;
            srcPixel += screenshot->PixelSize;
        }
        srcRow += screenshot->Pitch;
    }

    return outData;
}

} // namespace ScreenshotFormat

#endif // EMU_OVERLAY
