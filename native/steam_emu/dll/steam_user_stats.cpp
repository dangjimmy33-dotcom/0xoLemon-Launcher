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

#include "dll/steam_user_stats.h"
#include <cmath>
#include <random>


void Steam_User_Stats::steam_user_stats_network_low_level(void *object, Common_Message *msg)
{
    // PRINT_DEBUG_ENTRY();

    auto inst = (Steam_User_Stats *)object;
    inst->network_callback_low_level(msg);
}

void Steam_User_Stats::steam_user_stats_run_every_runcb(void *object)
{
    // PRINT_DEBUG_ENTRY();

    auto inst = (Steam_User_Stats *)object;
    inst->steam_run_callback();
}


Steam_User_Stats::Steam_User_Stats(Settings *settings, class Networking *network, Local_Storage *local_storage, class SteamCallResults *callback_results, class SteamCallBacks *callbacks, class RunEveryRunCB *run_every_runcb, Steam_Overlay* overlay):
    settings(settings),
    network(network),
    local_storage(local_storage),
    callback_results(callback_results),
    callbacks(callbacks),
    defined_achievements(nlohmann::json::object()),
    user_achievements(nlohmann::json::object()),
    run_every_runcb(run_every_runcb),
    overlay(overlay)
{
    load_achievements_db(); // steam_settings/achievements.json
    load_achievements(); // %appdata%/<emu saves folder>/<app id>/achievements.json

    // discard achievements without a "name"
    auto x = defined_achievements.begin();
    while (x != defined_achievements.end()) {
        if (!x->contains("name")) {
            x = defined_achievements.erase(x);
        } else {
            ++x;
        }
    }

    for (auto & it : defined_achievements) {
        try {
            std::string name = static_cast<std::string const&>(it["name"]);
            sorted_achievement_names.push_back(name);

            achievement_trigger trig{};
            try {
                trig.name = name;
                trig.value_operation = static_cast<std::string const&>(it["progress"]["value"]["operation"]);
                std::string stat_name = common_helpers::to_lower(static_cast<std::string const&>(it["progress"]["value"]["operand1"]));
                const auto &min_val_obj = it["progress"]["min_val"];
                std::string min_val = min_val_obj.is_number()
                    ? std::to_string(static_cast<double>(min_val_obj))
                    : static_cast<std::string const&>(min_val_obj);
                const auto &max_val_obj = it["progress"]["max_val"];
                std::string max_val = max_val_obj.is_number()
                    ? std::to_string(static_cast<double>(max_val_obj))
                    : static_cast<std::string const&>(max_val_obj);
                trig.min_value = min_val;
                trig.max_value = max_val;
                achievement_stat_trigger[stat_name].push_back(trig);
            } catch(...) {}
            
            // default initial values, will only be added if they don't exist already
            auto &user_ach = user_achievements[name]; // this will create a new json entry if the key didn't exist already
            user_ach.emplace("earned", false);
            user_ach.emplace("earned_time", static_cast<uint32>(0));
            // they will throw an exception for achievements with no progress
            try {
                uint32 progress_min = std::stoul(trig.min_value);
                uint32 progress_max = std::stoul(trig.max_value);
                // if the above lines didn't throw exception then add the values
                user_ach.emplace("progress", progress_min);
                user_ach.emplace("max_progress", progress_max);
            } catch(...) {}
        } catch(...) {}

        try {
            it["hidden"] = std::to_string(it["hidden"].get<int>());
        } catch(...) {}

        it["displayName"] = get_value_for_language(it, "displayName", settings->get_language());
        it["description"] = get_value_for_language(it, "description", settings->get_language());

        it["icon_handle"] = Settings::UNLOADED_IMAGE_HANDLE;
        it["icon_gray_handle"] = Settings::UNLOADED_IMAGE_HANDLE;
    }

    //TODO: not sure if the sort is actually case insensitive, ach names seem to be treated by steam as case insensitive so I assume they are.
    //need to find a game with achievements of different case names to confirm
    std::sort(sorted_achievement_names.begin(), sorted_achievement_names.end(), [](const std::string lhs, const std::string rhs){
        const auto result = std::mismatch(lhs.cbegin(), lhs.cend(), rhs.cbegin(), rhs.cend(), [](const unsigned char lhs, const unsigned char rhs){return std::tolower(lhs) == std::tolower(rhs);});
        return result.second != rhs.cend() && (result.first == lhs.cend() || std::tolower(*result.first) < std::tolower(*result.second));}
    );
    
    if (!settings->disable_sharing_stats_with_gameserver) {
        this->network->setCallback(CALLBACK_ID_GAMESERVER_STATS, settings->get_local_steam_id(), &Steam_User_Stats::steam_user_stats_network_stats, this);
    }
    if (settings->share_leaderboards_over_network) {
        this->network->setCallback(CALLBACK_ID_LEADERBOARDS_STATS, settings->get_local_steam_id(), &Steam_User_Stats::steam_user_stats_network_leaderboards, this);
    }
    this->network->setCallback(CALLBACK_ID_USER_STATS, settings->get_local_steam_id(), &Steam_User_Stats::steam_user_stats_network_stats, this);
    this->network->setCallback(CALLBACK_ID_USER_STATUS, settings->get_local_steam_id(), &Steam_User_Stats::steam_user_stats_network_low_level, this);
    this->run_every_runcb->add(&Steam_User_Stats::steam_user_stats_run_every_runcb, this);

    // Proactively start fetching Steam global achievement percentages and SteamHunters data
    // at construction time so cache is warm before the game or overlay ever requests it.
    RequestGlobalAchievementPercentages();
    RequestSteamHuntersData();
    RequestSteamCardExchangeData();

    oxo_achievement_pipe = OxoAchievementPipeAdapter::create(
        settings->get_local_game_id().AppID(),
        oxo_runtime_state(),
        [this](const nlohmann::json &command) {
            return oxo_execute_command(command);
        });
}

Steam_User_Stats::~Steam_User_Stats()
{
    // Join the adapter before any dependency captured by its command callback
    // starts being torn down.
    oxo_achievement_pipe.reset();

    if (!settings->disable_sharing_stats_with_gameserver) {
        this->network->rmCallback(CALLBACK_ID_GAMESERVER_STATS, settings->get_local_steam_id(), &Steam_User_Stats::steam_user_stats_network_stats, this);
    }
    if (settings->share_leaderboards_over_network) {
        this->network->rmCallback(CALLBACK_ID_LEADERBOARDS_STATS, settings->get_local_steam_id(), &Steam_User_Stats::steam_user_stats_network_leaderboards, this);
    }
    this->network->rmCallback(CALLBACK_ID_USER_STATS, settings->get_local_steam_id(), &Steam_User_Stats::steam_user_stats_network_stats, this);
    this->network->rmCallback(CALLBACK_ID_USER_STATUS, settings->get_local_steam_id(), &Steam_User_Stats::steam_user_stats_network_low_level, this);
    this->run_every_runcb->remove(&Steam_User_Stats::steam_user_stats_run_every_runcb, this);
}

nlohmann::json Steam_User_Stats::oxo_runtime_state()
{
    std::lock_guard<std::recursive_mutex> lock(global_mutex);

    nlohmann::json schema = nlohmann::json::array();
    for (const auto &defined : defined_achievements) {
        try {
            const auto id = defined.value("name", std::string{});
            if (id.empty()) continue;

            bool hidden = false;
            const auto hidden_it = defined.find("hidden");
            if (hidden_it != defined.end()) {
                if (hidden_it->is_boolean()) {
                    hidden = hidden_it->get<bool>();
                } else if (hidden_it->is_number_integer()) {
                    hidden = hidden_it->get<int64_t>() != 0;
                } else if (hidden_it->is_string()) {
                    const auto value = common_helpers::to_lower(hidden_it->get<std::string>());
                    hidden = value == "1" || value == "true";
                }
            }

            uint64_t target = 0;
            const auto progress_it = defined.find("progress");
            if (progress_it != defined.end() && progress_it->is_object()) {
                const auto max_it = progress_it->find("max_val");
                if (max_it != progress_it->end()) {
                    if (max_it->is_number_unsigned()) {
                        target = max_it->get<uint64_t>();
                    } else if (max_it->is_number_integer()) {
                        const auto value = max_it->get<int64_t>();
                        if (value > 0) target = static_cast<uint64_t>(value);
                    } else if (max_it->is_string()) {
                        target = std::stoull(max_it->get<std::string>());
                    }
                }
            }
            if (!target) {
                const auto user_it = user_achievements.find(id);
                if (user_it != user_achievements.end()) {
                    target = user_it->value("max_progress", uint64_t{});
                }
            }

            schema.push_back({
                {"id", id},
                {"name", defined.value("displayName", id)},
                {"description", defined.value("description", std::string{})},
                {"hidden", hidden},
                {"target", target},
            });
        } catch (...) {
            // A malformed optional schema field must not disable transport for
            // otherwise valid achievements.
        }
    }

    nlohmann::json achievements = nlohmann::json::object();
    if (user_achievements.is_object()) achievements = user_achievements;

    nlohmann::json stats = nlohmann::json::object();
    for (const auto &[name, value] : stats_cache_int) stats[name] = value;
    for (const auto &[name, value] : stats_cache_float) stats[name] = value;

    return {
        {"schema", std::move(schema)},
        {"achievements", std::move(achievements)},
        {"stats", std::move(stats)},
    };
}

nlohmann::json Steam_User_Stats::oxo_execute_command(const nlohmann::json &command)
{
    std::lock_guard<std::recursive_mutex> lock(global_mutex);

    const auto fail = [](const std::string &error) {
        return nlohmann::json{{"ok", false}, {"error", error}};
    };
    const auto succeed = [](nlohmann::json result = nlohmann::json::object()) {
        return nlohmann::json{{"ok", true}, {"result", std::move(result)}};
    };
    const auto valid_id = [](const std::string &value) {
        return !value.empty() && value.size() <= 512 &&
            std::none_of(value.begin(), value.end(), [](unsigned char ch) {
                return std::iscntrl(ch) != 0;
            });
    };

    if (!command.is_object()) return fail("Command must be a JSON object");
    const auto type = command.value("type", std::string{});

    if (type == "queryState") {
        auto state = oxo_runtime_state();
        if (oxo_achievement_pipe) {
            auto event = state;
            event["type"] = "schemaReady";
            oxo_achievement_pipe->emit_event(std::move(event));
        }
        return succeed(std::move(state));
    }

    if (type == "flush") {
        return StoreStats() ? succeed({{"stored", true}}) : fail("GSE rejected StoreStats");
    }

    if (type == "unlock" || type == "clear" || type == "setProgress") {
        const auto achievement_id = command.value("achievementId", std::string{});
        if (!valid_id(achievement_id)) return fail("Invalid achievementId");

        if (type == "unlock") {
            return SetAchievement(achievement_id.c_str())
                ? succeed({{"achievementId", achievement_id}, {"unlocked", true}})
                : fail("GSE could not unlock the achievement");
        }
        if (type == "clear") {
            return ClearAchievement(achievement_id.c_str())
                ? succeed({{"achievementId", achievement_id}, {"unlocked", false}})
                : fail("GSE could not clear the achievement");
        }

        uint64_t current = 0;
        uint64_t target = 0;
        try {
            current = command.at("current").get<uint64_t>();
            target = command.at("target").get<uint64_t>();
        } catch (...) {
            return fail("Progress values must be unsigned integers");
        }
        if (!target || current > target || target > UINT32_MAX) {
            return fail("Progress must satisfy 0 <= current <= target <= UINT32_MAX");
        }
        const bool applied = current == target
            ? SetAchievement(achievement_id.c_str())
            : IndicateAchievementProgress(
                achievement_id.c_str(),
                static_cast<uint32>(current),
                static_cast<uint32>(target));
        return applied
            ? succeed({{"achievementId", achievement_id}, {"current", current}, {"target", target}})
            : fail("GSE could not update achievement progress");
    }

    if (type == "setStat") {
        const auto stat_id = command.value("statId", std::string{});
        if (!valid_id(stat_id)) return fail("Invalid statId");

        double value = 0.0;
        try {
            value = command.at("value").get<double>();
        } catch (...) {
            return fail("Stat value must be numeric");
        }
        if (!std::isfinite(value)) return fail("Stat value must be finite");

        const bool integer = command.value("integer", false);
        bool applied = false;
        if (integer) {
            if (std::trunc(value) != value ||
                value < static_cast<double>(INT32_MIN) ||
                value > static_cast<double>(INT32_MAX)) {
                return fail("Integer stat value is outside the int32 range");
            }
            applied = SetStat(stat_id.c_str(), static_cast<int32>(value));
        } else {
            const auto max_float = static_cast<double>(std::numeric_limits<float>::max());
            if (value < -max_float || value > max_float) {
                return fail("Float stat value is outside the float range");
            }
            applied = SetStat(stat_id.c_str(), static_cast<float>(value));
        }
        return applied
            ? succeed({{"statId", stat_id}, {"value", value}, {"integer", integer}})
            : fail("GSE could not update the stat");
    }

    return fail("Unsupported achievement command type");
}


// Retrieves the number of players currently playing your game (online + offline)
// This call is asynchronous, with the result returned in NumberOfCurrentPlayers_t
STEAM_CALL_RESULT( NumberOfCurrentPlayers_t )
SteamAPICall_t Steam_User_Stats::GetNumberOfCurrentPlayers()
{
    PRINT_DEBUG_ENTRY();
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    
    std::random_device rd{};
    std::mt19937 gen(rd());
    std::uniform_int_distribution<int32> distrib(117, 1017);
 
    NumberOfCurrentPlayers_t data{};
    data.m_bSuccess = 1;
    data.m_cPlayers = distrib(gen);
    auto ret = callback_results->addCallResult(data.k_iCallback, &data, sizeof(data));
    callbacks->addCBResult(data.k_iCallback, &data, sizeof(data));
    return ret;
}



// --- old interface version

uint32 Steam_User_Stats::GetNumStats( CGameID nGameID )
{
    PRINT_DEBUG("old %llu", nGameID.ToUint64());
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return 0;
    }
    return (uint32)settings->getStats().size();
}

const char *Steam_User_Stats::GetStatName( CGameID nGameID, uint32 iStat )
{
    PRINT_DEBUG("old %llu [%u]", nGameID.ToUint64(), iStat);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);

    auto &stats = settings->getStats();
    if (settings->get_local_game_id() != nGameID || iStat >= stats.size()) {
        return "";
    }
    
    return std::next(stats.begin(), iStat)->first.c_str();
}

ESteamUserStatType Steam_User_Stats::GetStatType( CGameID nGameID, const char *pchName )
{
    PRINT_DEBUG("old %llu '%s'", nGameID.ToUint64(), pchName);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    
    if (settings->get_local_game_id() != nGameID || !pchName) {
        return ESteamUserStatType::k_ESteamUserStatTypeINVALID;
    }
    
    std::string stat_name(common_helpers::to_lower(pchName));
    const auto &stats = settings->getStats();
    auto stat_info = stats.find(stat_name);
    if (stats.end() == stat_info) {
        return ESteamUserStatType::k_ESteamUserStatTypeINVALID;
    }

    switch (stat_info->second.type)
    {
    case StatInfo::STAT_TYPE_INT: return ESteamUserStatType::k_ESteamUserStatTypeINT;
    case StatInfo::STAT_TYPE_FLOAT: return ESteamUserStatType::k_ESteamUserStatTypeFLOAT;
    case StatInfo::STAT_TYPE_AVGRATE: return ESteamUserStatType::k_ESteamUserStatTypeAVGRATE;
    
    default: PRINT_DEBUG("[X] unhandled type %i", (int)stat_info->second.type); break;
    }
    
    return ESteamUserStatType::k_ESteamUserStatTypeINVALID;
}

uint32 Steam_User_Stats::GetNumAchievements( CGameID nGameID )
{
    PRINT_DEBUG("old %llu", nGameID.ToUint64());
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return 0;
    }
    
    return GetNumAchievements();
}

const char *Steam_User_Stats::GetAchievementName( CGameID nGameID, uint32 iAchievement )
{
    PRINT_DEBUG("old %llu [%u]", nGameID.ToUint64(), iAchievement);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return "";
    }
    
    return GetAchievementName(iAchievement);
}

uint32 Steam_User_Stats::GetNumGroupAchievements( CGameID nGameID )
{
    PRINT_DEBUG("old %llu // TODO", nGameID.ToUint64());
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return 0;
    }
    
    return 0;
}

const char *Steam_User_Stats::GetGroupAchievementName( CGameID nGameID, uint32 iAchievement )
{
    PRINT_DEBUG("old %llu [%u] // TODO", nGameID.ToUint64(), iAchievement);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return "";
    }
    
    return "";
}

bool Steam_User_Stats::RequestCurrentStats( CGameID nGameID )
{
    PRINT_DEBUG("old %llu", nGameID.ToUint64());
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return false;
    }
    
    return RequestCurrentStats();
}

bool Steam_User_Stats::GetStat( CGameID nGameID, const char *pchName, int32 *pData )
{
    PRINT_DEBUG("old %llu '%s' %p", nGameID.ToUint64(), pchName, pData);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    
    if (pData) *pData = 0;
    if (settings->get_local_game_id() != nGameID) {
        return false;
    }
    
    return GetStat(pchName, pData);
}

bool Steam_User_Stats::GetStat( CGameID nGameID, const char *pchName, float *pData )
{
    PRINT_DEBUG("old %llu '%s' %p", nGameID.ToUint64(), pchName, pData);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    
    if (pData) *pData = 0;
    if (settings->get_local_game_id() != nGameID) {
        return false;
    }
    
    return GetStat(pchName, pData);
}

bool Steam_User_Stats::SetStat( CGameID nGameID, const char *pchName, int32 nData )
{
    PRINT_DEBUG("old %llu '%s' %i", nGameID.ToUint64(), pchName, nData);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return false;
    }
    
    return SetStat(pchName, nData);
}

bool Steam_User_Stats::SetStat( CGameID nGameID, const char *pchName, float fData )
{
    PRINT_DEBUG("old %llu '%s' %f", nGameID.ToUint64(), pchName, fData);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return false;
    }
    
    return SetStat(pchName, fData);
}

bool Steam_User_Stats::UpdateAvgRateStat( CGameID nGameID, const char *pchName, float flCountThisSession, double dSessionLength )
{
    PRINT_DEBUG("old %llu '%s' %f %f", nGameID.ToUint64(), pchName, flCountThisSession, dSessionLength);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return false;
    }
    
    return UpdateAvgRateStat(pchName, flCountThisSession, dSessionLength);
}

bool Steam_User_Stats::GetAchievement( CGameID nGameID, const char *pchName, bool *pbAchieved )
{
    PRINT_DEBUG("old %llu '%s' %p", nGameID.ToUint64(), pchName, pbAchieved);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    
    if (pbAchieved) *pbAchieved = false;
    if (settings->get_local_game_id() != nGameID) {
        return false;
    }
    
    return GetAchievement(pchName, pbAchieved);
}

bool Steam_User_Stats::GetGroupAchievement( CGameID nGameID, const char *pchName, bool *pbAchieved )
{
    PRINT_DEBUG("old %llu '%s' %p // TODO", nGameID.ToUint64(), pchName, pbAchieved);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    
    if (pbAchieved) *pbAchieved = false;
    if (settings->get_local_game_id() != nGameID) {
        return false;
    }
    
    return false;
}

bool Steam_User_Stats::SetAchievement( CGameID nGameID, const char *pchName )
{
    PRINT_DEBUG("old %llu '%s'", nGameID.ToUint64(), pchName);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID && settings->achievement_bypass) {
        return false;
    }
    
    return SetAchievement(pchName);
}

bool Steam_User_Stats::SetGroupAchievement( CGameID nGameID, const char *pchName )
{
    PRINT_DEBUG("old %llu '%s' // TODO", nGameID.ToUint64(), pchName);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return false;
    }
    
    return false;
}

bool Steam_User_Stats::StoreStats( CGameID nGameID )
{
    PRINT_DEBUG("old %llu", nGameID.ToUint64());
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return false;
    }
    
    return StoreStats();
}

bool Steam_User_Stats::ClearAchievement( CGameID nGameID, const char *pchName )
{
    PRINT_DEBUG("old %llu '%s'", nGameID.ToUint64(), pchName);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return false;
    }
    
    return ClearAchievement(pchName);
}

bool Steam_User_Stats::ClearGroupAchievement( CGameID nGameID, const char *pchName )
{
    PRINT_DEBUG("old %llu '%s' // TODO", nGameID.ToUint64(), pchName);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return 0;
    }
    
    return false;
}

int Steam_User_Stats::GetAchievementIcon( CGameID nGameID, const char *pchName )
{
    PRINT_DEBUG("old %llu '%s'", nGameID.ToUint64(), pchName);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return Settings::INVALID_IMAGE_HANDLE;
    }
    
    return GetAchievementIcon(pchName);
}

const char *Steam_User_Stats::GetAchievementDisplayAttribute( CGameID nGameID, const char *pchName, const char *pchKey )
{
    PRINT_DEBUG("old %llu '%s' ['%s']", nGameID.ToUint64(), pchName, pchKey);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return "";
    }
    
    return GetAchievementDisplayAttribute(pchName, pchKey);
}

bool Steam_User_Stats::IndicateAchievementProgress( CGameID nGameID, const char *pchName, uint32 nCurProgress, uint32 nMaxProgress )
{
    PRINT_DEBUG("old %llu '%s' %u %u", nGameID.ToUint64(), pchName, nCurProgress, nMaxProgress);
    std::lock_guard<std::recursive_mutex> lock(global_mutex);
    if (settings->get_local_game_id() != nGameID) {
        return false;
    }
    
    return IndicateAchievementProgress(pchName, nCurProgress, nMaxProgress);
}


// --- steam callbacks

void Steam_User_Stats::steam_run_callback()
{
    send_updated_stats();
    load_achievements_icons();
    send_pending_user_stats_requests();

    // once global percentages are fetched, push them to overlay once (for display + optional sort)
    // and persist them into the user achievements.json
    if (global_achievement_percentages_populated && !global_achievement_percentages_overlay_sorted) {
        if (overlay) {
            overlay->SortAchievementsByGlobalPercent(global_achievement_percentages);
        }

        // write global_percent into every achievement entry and save to disk
        bool changed = false;
        for (auto &kv : global_achievement_percentages) {
            float existing = user_achievements[kv.first].value("global_percent", -1.0f);
            if (existing != kv.second) {
                user_achievements[kv.first]["global_percent"] = kv.second;
                changed = true;
            }
        }
        if (changed) save_achievements();

        global_achievement_percentages_overlay_sorted = true;
    }
}



// --- networking callbacks
// only triggered when we have a message

// user connect/disconnect
void Steam_User_Stats::network_callback_low_level(Common_Message *msg)
{
    CSteamID steamid((uint64)msg->source_id());
    // this should never happen, but just in case
    if (steamid == settings->get_local_steam_id()) return;

    switch (msg->low_level().type())
    {
    case Low_Level::CONNECT:
        // nothing
    break;
    
    case Low_Level::DISCONNECT: {
        for (auto &board : cached_leaderboards) {
            board.remove_entries(steamid);
        }
        
        // TODO: need tests on real steam
        bool had_data = false;

        auto it_res_r = received_user_stats_data.find(steamid.ConvertToUint64());
        if (it_res_r != received_user_stats_data.end()) {
            had_data = true;
            it_res_r = received_user_stats_data.erase(it_res_r);
        }
        auto it_res_p = pending_user_stats_requests.find(steamid.ConvertToUint64());
        if (it_res_p != pending_user_stats_requests.end()) {
            trigger_user_stats_received(steamid, it_res_p->second.api_id);
            it_res_p = pending_user_stats_requests.erase(it_res_p);
        }
        else if (had_data) {
            UserStatsUnloaded_t data{};
            data.m_steamIDUser = steamid;
            callbacks->addCBResult(data.k_iCallback, &data, sizeof(data), 0.0);
        }

        // PRINT_DEBUG("removed user %llu", (uint64)steamid.ConvertToUint64());
    }
    break;
    
    default:
        PRINT_DEBUG("unknown type %i", (int)msg->low_level().type());
    break;
    }
}
