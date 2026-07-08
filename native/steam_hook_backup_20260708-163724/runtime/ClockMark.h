// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#pragma once

#include <chrono>

namespace ClockMark {

    class Span {
    public:
        Span() : start_(std::chrono::steady_clock::now()) {}

        double Ms() const {
            return std::chrono::duration<double, std::milli>(
                std::chrono::steady_clock::now() - start_).count();
        }

    private:
        std::chrono::steady_clock::time_point start_;
    };

}
