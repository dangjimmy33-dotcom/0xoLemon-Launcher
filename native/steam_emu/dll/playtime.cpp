/* Copyright (C) 2019 Mr Goldberg
   This file is part of the Goldberg Emulator

   The Goldberg Emulator is free software; you can redistribute it and/or
   modify it under the terms of the GNU Lesser General Public
   License as published by the Free Software Foundation; either
   version 3 of the License, or (at your option) any later version.

   The Goldberg Emulator is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
   Lesser General Public License for more details.

   You should have received a copy of the GNU Lesser General Public
   License along with the Goldberg Emulator; if not, see
   <http://www.gnu.org/licenses/>.  */

#include "dll/playtime.h"

#include <limits>

PlaytimeCounter::PlaytimeCounter(Local_Storage* local_storage, bool record_playtime)
   : local_storage(local_storage),
     record_playtime(record_playtime),
     last_tick(std::chrono::steady_clock::now())
{
    load();
    if (record_playtime) {
        save();
    }
}

PlaytimeCounter::~PlaytimeCounter()
{
    save();
}

void PlaytimeCounter::tick()
{
    auto now = std::chrono::steady_clock::now();

    if (!initialized) {
        load();
        std::lock_guard<std::mutex> lock(mutex);
        last_tick = now;
        initialized = true;
        return;
    }

    bool need_save = false;
    {
        std::lock_guard<std::mutex> lock(mutex);

        auto delta_ms = std::chrono::duration_cast<std::chrono::milliseconds>(now - last_tick).count();
        if (delta_ms <= 0) return;

        last_tick = now;

        // Accumulate milliseconds, convert whole seconds
        playtime_accumulator_ms += static_cast<uint64_t>(delta_ms);
        uint64_t accrued_sec = playtime_accumulator_ms / 1000;
        playtime_accumulator_ms %= 1000;

        if (accrued_sec > 0) {
            // Accumulate total playtime (unless paused)
            if (!pause_total) {
                const uint64_t maxv = std::numeric_limits<uint64_t>::max();
                if (playtime_seconds > maxv - accrued_sec) {
                    playtime_seconds = maxv;
                } else {
                    playtime_seconds += accrued_sec;
                }

                since_save += accrued_sec;
                if (since_save >= 15) {
                    since_save = 0;
                    need_save = true;
                }
            }

            // Accumulate session playtime (unless paused)
            if (!pause_session) {
                session_seconds_accumulated += accrued_sec;
            }
        }
    }

    if (need_save) {
        save();
    }
}

void PlaytimeCounter::load()
{
    std::lock_guard<std::mutex> lock(mutex);

    playtime_seconds = 0;
    playtime_accumulator_ms = 0;
    session_seconds_accumulated = 0;

    std::string data(32, '\0');
    if (local_storage->get_data("", playtime_filename, data.data(), static_cast<unsigned int>(data.size()), 0) > 0) {
        try {
            playtime_seconds = std::stoull(data);
        } catch (...) {}
    }

    initialized = true;
}

void PlaytimeCounter::save()
{
    if (!record_playtime) return;
    std::lock_guard<std::mutex> lock(mutex);

    std::string data = std::to_string(playtime_seconds);
    local_storage->store_data("", playtime_filename, data.data(), static_cast<unsigned int>(data.size()));
}

uint64_t PlaytimeCounter::seconds() const
{
    std::lock_guard<std::mutex> lock(mutex);
    return playtime_seconds;
}

uint64_t PlaytimeCounter::session_seconds() const
{
    std::lock_guard<std::mutex> lock(mutex);
    return session_seconds_accumulated;
}

void PlaytimeCounter::set_pause_total(bool pause)
{
    std::lock_guard<std::mutex> lock(mutex);
    pause_total = pause;
}

void PlaytimeCounter::set_pause_session(bool pause)
{
    std::lock_guard<std::mutex> lock(mutex);
    pause_session = pause;
}
