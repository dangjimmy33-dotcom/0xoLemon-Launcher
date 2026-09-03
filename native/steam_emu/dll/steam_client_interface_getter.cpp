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

#include "dll/steam_client.h"

#if defined(__WINDOWS__)
#include <TlHelp32.h>
#endif


// retrieves the ISteamBilling interface associated with the handle
ISteamBilling *Steam_Client::GetISteamBilling( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return nullptr;

    if (strcmp(pchVersion, "SteamBilling001") == 0) {
        return nullptr; // real steamclient64.dll returns null
    } else if (strcmp(pchVersion, STEAMBILLING_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamBilling *>(static_cast<ISteamBilling *>(steam_billing));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

void *Steam_Client::GetISteamBilling_old( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("old");
    return GetISteamBilling(hSteamUser, hSteamPipe, pchVersion);
}

// retrieves the ISteamAppDisableUpdate interface associated with the handle
ISteamAppDisableUpdate *Steam_Client::GetISteamAppDisableUpdate( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return nullptr;

    if (strcmp(pchVersion, STEAMAPPDISABLEUPDATE_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamAppDisableUpdate *>(static_cast<ISteamAppDisableUpdate *>(steam_app_disable_update));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// retrieves the ISteamTimeline interface associated with the handle
ISteamTimeline *Steam_Client::GetISteamTimeline( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return nullptr;

    if (strcmp(pchVersion, "STEAMTIMELINE_INTERFACE_V001") == 0) {
        return reinterpret_cast<ISteamTimeline *>(static_cast<ISteamTimeline001 *>(steam_timeline));
    } else if (strcmp(pchVersion, "STEAMTIMELINE_INTERFACE_V002") == 0) {
        return reinterpret_cast<ISteamTimeline *>(static_cast<ISteamTimeline002 *>(steam_timeline));
    } else if (strcmp(pchVersion, "STEAMTIMELINE_INTERFACE_V003") == 0) {
        return reinterpret_cast<ISteamTimeline *>(static_cast<ISteamTimeline003 *>(steam_timeline));
    } else if (strcmp(pchVersion, STEAMTIMELINE_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamTimeline *>(static_cast<ISteamTimeline *>(steam_timeline));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// retrieves the ISteamGameStats interface associated with the handle
ISteamGameStats *Steam_Client::GetISteamGameStats( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return nullptr;

    Steam_GameStats *steam_gamestats_tmp{};

    if (steam_pipes[hSteamPipe].type == Steam_Pipe_Type::SERVER) {
        steam_gamestats_tmp = steam_gameserver_gamestats;
    } else {
        steam_gamestats_tmp = steam_gamestats;
    }

    if (strcmp(pchVersion, STEAMGAMESTATS_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamGameStats *>(static_cast<ISteamGameStats *>(steam_gamestats_tmp));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// retrieves the ISteamUser interface associated with the handle
ISteamUser *Steam_Client::GetISteamUser( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return nullptr;

    Steam_User *steam_user_tmp{};

    if (steam_pipes[hSteamPipe].type == Steam_Pipe_Type::SERVER) {
        steam_user_tmp = steam_gameserver_user;
    } else {
        steam_user_tmp = steam_user;
    }

    if (strcmp(pchVersion, "SteamUser001") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser001 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, "SteamUser002") == 0) {
        // Ugh...
        if (steamclient_version < 2) {
            return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser002_old *>(steam_user_tmp));
        } else {
            return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser002 *>(steam_user_tmp));
        }
    } else if (strcmp(pchVersion, "SteamUser004") == 0) {
        // Double ugh...
        if (steamclient_version < 5) {
            return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser004_old *>(steam_user_tmp));
        } else {
            return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser004 *>(steam_user_tmp)); // sdk 0.99u
        }
    } else if (strcmp(pchVersion, "SteamUser005") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser005 *>(steam_user_tmp)); // sdk 0.99v
    } else if (strcmp(pchVersion, "SteamUser006") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser006 *>(steam_user_tmp)); // sdk 0.99w
    } else if (strcmp(pchVersion, "SteamUser007") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser007 *>(steam_user_tmp)); // sdk 0.99x
    } else if (strcmp(pchVersion, "SteamUser008") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser008 *>(steam_user_tmp)); // sdk 0.99y
    } else if (strcmp(pchVersion, "SteamUser009") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser009 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, "SteamUser010") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser010 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, "SteamUser011") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser011 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, "SteamUser012") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser012 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, "SteamUser013") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser013 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, "SteamUser014") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser014 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, "SteamUser015") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser015 *>(steam_user_tmp)); // SteamUser015 Not found in public Archive, must be between 1.12-1.13
    } else if (strcmp(pchVersion, "SteamUser016") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser016 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, "SteamUser017") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser017 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, "SteamUser018") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser018 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, "SteamUser019") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser019 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, "SteamUser020") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser020 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, "SteamUser021") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser021 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, "SteamUser022") == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser022 *>(steam_user_tmp));
    } else if (strcmp(pchVersion, STEAMUSER_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamUser *>(static_cast<ISteamUser *>(steam_user_tmp));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// retrieves the ISteamGameServer interface associated with the handle
ISteamGameServer *Steam_Client::GetISteamGameServer( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;


    if (strcmp(pchVersion, "SteamGameServer001") == 0) {
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer001 *>(steam_gameserver));
    } else if (strcmp(pchVersion, "SteamGameServer002") == 0) {
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer002 *>(steam_gameserver)); // not found in public archives, from proton repo src
    } else if (strcmp(pchVersion, "SteamGameServer003") == 0) {
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer003 *>(steam_gameserver)); // not found in public archives, from proton repo src
    } else if (strcmp(pchVersion, "SteamGameServer004") == 0) {
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer004 *>(steam_gameserver));
    } else if (strcmp(pchVersion, "SteamGameServer005") == 0) {
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer005 *>(steam_gameserver));
    } else if (strcmp(pchVersion, "SteamGameServer006") == 0) { // Not found in public Archive, defined as an alias to v008 in proton src
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer008 *>(steam_gameserver)); // SteamGameServer006 Not exists
    } else if (strcmp(pchVersion, "SteamGameServer007") == 0) { // Not found in public Archive, defined as an alias to v008 in proton src
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer008 *>(steam_gameserver)); // SteamGameServer007 Not exists
    } else if (strcmp(pchVersion, "SteamGameServer008") == 0) {
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer008 *>(steam_gameserver));
    } else if (strcmp(pchVersion, "SteamGameServer009") == 0) {
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer009 *>(steam_gameserver));
    } else if (strcmp(pchVersion, "SteamGameServer010") == 0) {
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer010 *>(steam_gameserver));
    } else if (strcmp(pchVersion, "SteamGameServer011") == 0) {
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer011 *>(steam_gameserver));
    } else if (strcmp(pchVersion, "SteamGameServer012") == 0) {
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer012 *>(steam_gameserver));
    }

    gameserver_has_ipv6_functions = true;
    if (strcmp(pchVersion, "SteamGameServer013") == 0) {
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer013 *>(steam_gameserver));
    } else if (strcmp(pchVersion, "SteamGameServer014") == 0) {
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer014 *>(steam_gameserver));
    } else if (strcmp(pchVersion, STEAMGAMESERVER_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamGameServer *>(static_cast<ISteamGameServer *>(steam_gameserver));
    }
    
    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// returns the ISteamFriends interface
ISteamFriends *Steam_Client::GetISteamFriends( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;

    if (strcmp(pchVersion, "SteamFriends001") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends001 *>(steam_friends)); // sdk 0.99u
    } else if (strcmp(pchVersion, "SteamFriends002") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends002 *>(steam_friends)); // sdk 0.99y
    } else if (strcmp(pchVersion, "SteamFriends003") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends003 *>(steam_friends));
    } else if (strcmp(pchVersion, "SteamFriends004") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends004 *>(steam_friends));
    } else if (strcmp(pchVersion, "SteamFriends005") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends005 *>(steam_friends));
    } else if (strcmp(pchVersion, "SteamFriends006") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends006 *>(steam_friends));
    } else if (strcmp(pchVersion, "SteamFriends007") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends007 *>(steam_friends));
    } else if (strcmp(pchVersion, "SteamFriends008") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends008 *>(steam_friends));
    } else if (strcmp(pchVersion, "SteamFriends009") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends009 *>(steam_friends));
    } else if (strcmp(pchVersion, "SteamFriends010") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends010 *>(steam_friends)); // SteamFriends010 Not found in public Archive, must be between 1.16-1.17
    } else if (strcmp(pchVersion, "SteamFriends011") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends011 *>(steam_friends));
    } else if (strcmp(pchVersion, "SteamFriends012") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends012 *>(steam_friends)); // SteamFriends012 Not found in public Archive, must be between 1.19-1.20
    } else if (strcmp(pchVersion, "SteamFriends013") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends013 *>(steam_friends));
    } else if (strcmp(pchVersion, "SteamFriends014") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends014 *>(steam_friends));
    } else if (strcmp(pchVersion, "SteamFriends015") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends015 *>(steam_friends));
    } else if (strcmp(pchVersion, "SteamFriends016") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends016 *>(steam_friends)); // SteamFriends016 Not found in public Archive, must be between 1.42-1.43
    } else if (strcmp(pchVersion, "SteamFriends017") == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends017 *>(steam_friends));
    } else if (strcmp(pchVersion, STEAMFRIENDS_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamFriends *>(static_cast<ISteamFriends *>(steam_friends));
    }
    
    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// returns the ISteamUtils interface
ISteamUtils *Steam_Client::GetISteamUtils( HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe)) return NULL;

    Steam_Utils *steam_utils_temp{};

    if (steam_pipes[hSteamPipe].type == Steam_Pipe_Type::SERVER) {
        steam_utils_temp = steam_gameserver_utils;
    } else {
        steam_utils_temp = steam_utils;
    }


    if (strcmp(pchVersion, "SteamUtils001") == 0) {
        return reinterpret_cast<ISteamUtils *>(static_cast<ISteamUtils001 *>(steam_utils_temp));
    } else if (strcmp(pchVersion, "SteamUtils002") == 0) {
        return reinterpret_cast<ISteamUtils *>(static_cast<ISteamUtils002 *>(steam_utils_temp));
    } else if (strcmp(pchVersion, "SteamUtils003") == 0) {
        return reinterpret_cast<ISteamUtils *>(static_cast<ISteamUtils003 *>(steam_utils_temp)); // ISteamUtils003 Not found in public Archive, must be between 1.02-1.03
    } else if (strcmp(pchVersion, "SteamUtils004") == 0) {
        return reinterpret_cast<ISteamUtils *>(static_cast<ISteamUtils004 *>(steam_utils_temp));
    } else if (strcmp(pchVersion, "SteamUtils005") == 0) {
        return reinterpret_cast<ISteamUtils *>(static_cast<ISteamUtils005 *>(steam_utils_temp));
    } else if (strcmp(pchVersion, "SteamUtils006") == 0) {
        return reinterpret_cast<ISteamUtils *>(static_cast<ISteamUtils006 *>(steam_utils_temp));
    } else if (strcmp(pchVersion, "SteamUtils007") == 0) {
        return reinterpret_cast<ISteamUtils *>(static_cast<ISteamUtils007 *>(steam_utils_temp));
    } else if (strcmp(pchVersion, "SteamUtils008") == 0) {
        return reinterpret_cast<ISteamUtils *>(static_cast<ISteamUtils008 *>(steam_utils_temp));
    } else if (strcmp(pchVersion, "SteamUtils009") == 0) {
        return reinterpret_cast<ISteamUtils *>(static_cast<ISteamUtils009 *>(steam_utils_temp));
    } else if (strcmp(pchVersion, "SteamUtils010") == 0) {
        return reinterpret_cast<ISteamUtils *>(static_cast<ISteamUtils010 *>(steam_utils_temp));
    } else if (strcmp(pchVersion, STEAMUTILS_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamUtils *>(static_cast<ISteamUtils *>(steam_utils_temp));
    }
    
    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// returns the ISteamMatchmaking interface
ISteamMatchmaking *Steam_Client::GetISteamMatchmaking( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;

    if (strcmp(pchVersion, "SteamMatchMaking001") == 0) { // SteamMatchMaking001 Not found in public Archive, from proton src
        return reinterpret_cast<ISteamMatchmaking *>(static_cast<ISteamMatchmaking001 *>(steam_matchmaking));
    } else if (strcmp(pchVersion, "SteamMatchMaking002") == 0) {
        return reinterpret_cast<ISteamMatchmaking *>(static_cast<ISteamMatchmaking002 *>(steam_matchmaking));
    } else if (strcmp(pchVersion, "SteamMatchMaking003") == 0) {
        return reinterpret_cast<ISteamMatchmaking *>(static_cast<ISteamMatchmaking003 *>(steam_matchmaking)); // SteamMatchMaking003 Not found in public Archive, must be between 1.01-1.02
    } else if (strcmp(pchVersion, "SteamMatchMaking004") == 0) {
        return reinterpret_cast<ISteamMatchmaking *>(static_cast<ISteamMatchmaking004 *>(steam_matchmaking));
    } else if (strcmp(pchVersion, "SteamMatchMaking005") == 0) {
        return reinterpret_cast<ISteamMatchmaking *>(static_cast<ISteamMatchmaking005 *>(steam_matchmaking)); // SteamMatchMaking005 Not found in public Archive, must be between 1.02-1.03
    } else if (strcmp(pchVersion, "SteamMatchMaking006") == 0) {
        return reinterpret_cast<ISteamMatchmaking *>(static_cast<ISteamMatchmaking006 *>(steam_matchmaking));
    } else if (strcmp(pchVersion, "SteamMatchMaking007") == 0) {
        return reinterpret_cast<ISteamMatchmaking *>(static_cast<ISteamMatchmaking007 *>(steam_matchmaking));
    } else if (strcmp(pchVersion, "SteamMatchMaking008") == 0) {
        return reinterpret_cast<ISteamMatchmaking *>(static_cast<ISteamMatchmaking008 *>(steam_matchmaking));
    } else if (strcmp(pchVersion, STEAMMATCHMAKING_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamMatchmaking *>(static_cast<ISteamMatchmaking *>(steam_matchmaking));
    }
    
    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// returns the ISteamMatchmakingServers interface
ISteamMatchmakingServers *Steam_Client::GetISteamMatchmakingServers( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;

    if (strcmp(pchVersion, "SteamMatchMakingServers001") == 0) {
        return reinterpret_cast<ISteamMatchmakingServers *>(static_cast<ISteamMatchmakingServers001 *>(steam_matchmaking_servers));
    } else if (strcmp(pchVersion, "SteamMatchMakingServers002") == 0) {
        return reinterpret_cast<ISteamMatchmakingServers *>(static_cast<ISteamMatchmakingServers002 *>(steam_matchmaking_servers));
    } else if (strcmp(pchVersion, STEAMMATCHMAKINGSERVERS_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamMatchmakingServers *>(static_cast<ISteamMatchmakingServers *>(steam_matchmaking_servers));
    }
    
    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// returns the a generic interface
void *Steam_Client::GetISteamGenericInterface( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("'%s' %i %i", pchVersion, hSteamUser, hSteamPipe);
    if (!steam_pipes.count(hSteamPipe)) return NULL;

    bool server = false;
    if (steam_pipes[hSteamPipe].type == Steam_Pipe_Type::SERVER) {
        // PRINT_DEBUG("requesting interface with server pipe");
        server = true;
    } else {
        // PRINT_DEBUG("requesting interface with client pipe");
        // if this is a user pipe, and version != "SteamNetworkingUtils", and version != "SteamUtils"
        if ((strstr(pchVersion, "SteamNetworkingUtils") != pchVersion) && (strstr(pchVersion, "SteamUtils") != pchVersion)) {
            if (!hSteamUser) return NULL;
        }
    }

    // !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
    // NOTE: you must try to read the one with the most characters first
    // !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!

    if (strstr(pchVersion, "SteamNetworkingSocketsSerialized") == pchVersion) {
        Steam_Networking_Sockets_Serialized *steam_networking_sockets_serialized_temp{};
        if (server) {
            steam_networking_sockets_serialized_temp = steam_gameserver_networking_sockets_serialized;
        } else {
            steam_networking_sockets_serialized_temp = steam_networking_sockets_serialized;
        }

        if (strcmp(pchVersion, "SteamNetworkingSocketsSerialized001") == 0) { // not found in public archives, defined as an alias to v002 in proton src
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSocketsSerialized002 *>(steam_networking_sockets_serialized_temp));
        } else if (strcmp(pchVersion, "SteamNetworkingSocketsSerialized002") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSocketsSerialized002 *>(steam_networking_sockets_serialized_temp));
        } else if (strcmp(pchVersion, "SteamNetworkingSocketsSerialized003") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSocketsSerialized003 *>(steam_networking_sockets_serialized_temp));
        } else if (strcmp(pchVersion, "SteamNetworkingSocketsSerialized004") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSocketsSerialized004 *>(steam_networking_sockets_serialized_temp));
        } else if (strcmp(pchVersion, "SteamNetworkingSocketsSerialized005") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSocketsSerialized005 *>(steam_networking_sockets_serialized_temp));
        }
    } else if (strstr(pchVersion, "SteamNetworkingSockets") == pchVersion) {
        Steam_Networking_Sockets *steam_networking_sockets_temp{};
        if (server) {
            steam_networking_sockets_temp = steam_gameserver_networking_sockets;
        } else {
            steam_networking_sockets_temp = steam_networking_sockets;
        }

        steam_networking_sockets_temp->set_version(pchVersion);

        if (strcmp(pchVersion, "SteamNetworkingSockets001") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSockets001 *>( steam_networking_sockets_temp)); // SteamNetworkingSockets001 Not found in public Archive, must be before 1.44
        } else if (strcmp(pchVersion, "SteamNetworkingSockets002") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSockets002 *>( steam_networking_sockets_temp));
        } else if (strcmp(pchVersion, "SteamNetworkingSockets003") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSockets003 *>( steam_networking_sockets_temp));
        } else if (strcmp(pchVersion, "SteamNetworkingSockets004") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSockets004 *>( steam_networking_sockets_temp));
        // TODO SteamNetworkingSockets005 not found in public archives
        } else if (strcmp(pchVersion, "SteamNetworkingSockets006") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSockets006 *>( steam_networking_sockets_temp));
        } else if (strcmp(pchVersion, "SteamNetworkingSockets007") == 0) { // Not found in public Archive, real steamclient64.dll returns null
            return nullptr;
        } else if (strcmp(pchVersion, "SteamNetworkingSockets008") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSockets008 *>( steam_networking_sockets_temp));
        } else if (strcmp(pchVersion, "SteamNetworkingSockets009") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSockets009 *>( steam_networking_sockets_temp));
        } else if (strcmp(pchVersion, "SteamNetworkingSockets010") == 0) { // Not found in public Archive, based on reversing
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSockets010 *>( steam_networking_sockets_temp));
        } else if (strcmp(pchVersion, "SteamNetworkingSockets011") == 0) { // Not found in public Archive, based on reversing, requested by appid 1492070
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSockets011 *>( steam_networking_sockets_temp));
        } else if (strcmp(pchVersion, "SteamNetworkingSockets012") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSockets012 *>( steam_networking_sockets_temp));
        } else if (strcmp(pchVersion, STEAMNETWORKINGSOCKETS_INTERFACE_VERSION) == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingSockets *>( steam_networking_sockets_temp));
        }
    } else if (strstr(pchVersion, "SteamNetworkingMessages") == pchVersion) {
        Steam_Networking_Messages *steam_networking_messages_temp{};
        if (server) {
            steam_networking_messages_temp = steam_gameserver_networking_messages;
        } else {
            steam_networking_messages_temp = steam_networking_messages;
        }

        if (strcmp(pchVersion, STEAMNETWORKINGMESSAGES_INTERFACE_VERSION) == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingMessages *>(steam_networking_messages_temp));
        }
    } else if (strstr(pchVersion, "SteamNetworkingUtils") == pchVersion) {
        if (strcmp(pchVersion, "SteamNetworkingUtils001") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingUtils001 *>(steam_networking_utils));
        } else if (strcmp(pchVersion, "SteamNetworkingUtils002") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingUtils002 *>(steam_networking_utils));
        } else if (strcmp(pchVersion, "SteamNetworkingUtils003") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingUtils003 *>(steam_networking_utils));
        } else if (strcmp(pchVersion, STEAMNETWORKINGUTILS_INTERFACE_VERSION) == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamNetworkingUtils *>(steam_networking_utils));
        }
    } else if (strstr(pchVersion, "SteamNetworking") == pchVersion) {
        return GetISteamNetworking(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamGameCoordinator") == pchVersion) {
        Steam_Game_Coordinator *steam_game_coordinator_temp{};
        if (server) {
            steam_game_coordinator_temp = steam_gameserver_game_coordinator;
        } else {
            steam_game_coordinator_temp = steam_game_coordinator;
        }

        if (strcmp(pchVersion, STEAMGAMECOORDINATOR_INTERFACE_VERSION) == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamGameCoordinator *>(steam_game_coordinator_temp));
        }
    } else if (strstr(pchVersion, "STEAMTV_INTERFACE_V") == pchVersion) {
        if (strcmp(pchVersion, STEAMTV_INTERFACE_VERSION) == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamTV *>(steam_tv));
        }
    } else if (strstr(pchVersion, "STEAMUSERITEMS_INTERFACE_VERSION") == pchVersion) {
        if (strcmp(pchVersion, "STEAMUSERITEMS_INTERFACE_VERSION001") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamUserItems001 *>(steam_user_items));
        } else if (strcmp(pchVersion, "STEAMUSERITEMS_INTERFACE_VERSION002") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamUserItems002 *>(steam_user_items));
        } else if (strcmp(pchVersion, "STEAMUSERITEMS_INTERFACE_VERSION003") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamUserItems003 *>(steam_user_items));
        } else if (strcmp(pchVersion, STEAMUSERITEMS_INTERFACE_VERSION) == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamUserItems *>(steam_user_items));
        }
    } else if (strstr(pchVersion, "STEAMGAMESERVERITEMS_INTERFACE_VERSION") == pchVersion) {
        if (strcmp(pchVersion, "STEAMGAMESERVERITEMS_INTERFACE_VERSION001") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamGameServerItems001 *>(steam_gameserver_items));
        } else if (strcmp(pchVersion, "STEAMGAMESERVERITEMS_INTERFACE_VERSION002") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamGameServerItems002 *>(steam_gameserver_items));
        } else if (strcmp(pchVersion, "STEAMGAMESERVERITEMS_INTERFACE_VERSION003") == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamGameServerItems003 *>(steam_gameserver_items));
        } else if (strcmp(pchVersion, STEAMGAMESERVERITEMS_INTERFACE_VERSION) == 0) {
            return reinterpret_cast<void *>(static_cast<ISteamGameServerItems *>(steam_gameserver_items));
        }
    } else if (strstr(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION") == pchVersion) {
        return GetISteamRemoteStorage(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamGameServerStats") == pchVersion) {
        return GetISteamGameServerStats(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamGameServer") == pchVersion) {
        return GetISteamGameServer(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamGameStats") == pchVersion) {
        return GetISteamGameStats(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamMatchMakingServers") == pchVersion) {
        return GetISteamMatchmakingServers(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamMatchMaking") == pchVersion) {
        return GetISteamMatchmaking(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamFriends") == pchVersion) {
        return GetISteamFriends(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamController") == pchVersion || strstr(pchVersion, "STEAMCONTROLLER_INTERFACE_VERSION") == pchVersion) {
        return GetISteamController(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMUGC_INTERFACE_VERSION") == pchVersion) {
        return GetISteamUGC(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMINVENTORY_INTERFACE") == pchVersion) {
        return GetISteamInventory(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMUSERSTATS_INTERFACE_VERSION") == pchVersion) {
        return GetISteamUserStats(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamUser") == pchVersion) {
        return GetISteamUser(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamUtils") == pchVersion) {
        return GetISteamUtils(hSteamPipe, pchVersion);
    } else if ((strstr(pchVersion, "STEAMAPPS_INTERFACE_VERSION") == pchVersion) || (strstr(pchVersion, "SteamApps") == pchVersion)) {
        return GetISteamApps(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMSCREENSHOTS_INTERFACE_VERSION") == pchVersion) {
        return GetISteamScreenshots(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMHTTP_INTERFACE_VERSION") == pchVersion) {
        return GetISteamHTTP(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMUNIFIEDMESSAGES_INTERFACE_VERSION") == pchVersion) {
        return DEPRECATED_GetISteamUnifiedMessages(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMAPPLIST_INTERFACE_VERSION") == pchVersion) {
        return GetISteamAppList(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMMUSIC_INTERFACE_VERSION") == pchVersion) {
        return GetISteamMusic(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMMUSICREMOTE_INTERFACE_VERSION") == pchVersion) {
        return GetISteamMusicRemote(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMHTMLSURFACE_INTERFACE_VERSION") == pchVersion) {
        return GetISteamHTMLSurface(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMVIDEO_INTERFACE") == pchVersion) {
        return GetISteamVideo(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamMasterServerUpdater") == pchVersion) {
        return GetISteamMasterServerUpdater(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamMatchGameSearch") == pchVersion) {
        return GetISteamGameSearch(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamParties") == pchVersion) {
        return GetISteamParties(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamInput") == pchVersion) {
        return GetISteamInput(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMREMOTEPLAY_INTERFACE_VERSION") == pchVersion) {
        return GetISteamRemotePlay(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMPARENTALSETTINGS_INTERFACE_VERSION") == pchVersion) {
        return GetISteamParentalSettings(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMAPPTICKET_INTERFACE_VERSION") == pchVersion) {
        return GetAppTicket(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "STEAMTIMELINE_INTERFACE") == pchVersion) {
        return GetISteamTimeline(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamAppDisableUpdate") == pchVersion) {
        return GetISteamAppDisableUpdate(hSteamUser, hSteamPipe, pchVersion);
    } else if (strstr(pchVersion, "SteamBilling") == pchVersion) {
        return GetISteamBilling(hSteamUser, hSteamPipe, pchVersion);
    }

    PRINT_DEBUG("No interface: %s", pchVersion);
    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// returns the ISteamUserStats interface
ISteamUserStats *Steam_Client::GetISteamUserStats( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;

    if (strcmp(pchVersion, "STEAMUSERSTATS_INTERFACE_VERSION001") == 0) {
        return reinterpret_cast<ISteamUserStats *>(static_cast<ISteamUserStats001 *>(steam_user_stats));
    } else if (strcmp(pchVersion, "STEAMUSERSTATS_INTERFACE_VERSION002") == 0) {
        return reinterpret_cast<ISteamUserStats *>(static_cast<ISteamUserStats002 *>(steam_user_stats));
    } else if (strcmp(pchVersion, "STEAMUSERSTATS_INTERFACE_VERSION003") == 0) {
        return reinterpret_cast<ISteamUserStats *>(static_cast<ISteamUserStats003 *>(steam_user_stats));
    } else if (strcmp(pchVersion, "STEAMUSERSTATS_INTERFACE_VERSION004") == 0) {
        return reinterpret_cast<ISteamUserStats *>(static_cast<ISteamUserStats004 *>(steam_user_stats));
    } else if (strcmp(pchVersion, "STEAMUSERSTATS_INTERFACE_VERSION005") == 0) {
        return reinterpret_cast<ISteamUserStats *>(static_cast<ISteamUserStats005 *>(steam_user_stats));
    } else if (strcmp(pchVersion, "STEAMUSERSTATS_INTERFACE_VERSION006") == 0) {
        return reinterpret_cast<ISteamUserStats *>(static_cast<ISteamUserStats006 *>(steam_user_stats));
    } else if (strcmp(pchVersion, "STEAMUSERSTATS_INTERFACE_VERSION007") == 0) {
        return reinterpret_cast<ISteamUserStats *>(static_cast<ISteamUserStats007 *>(steam_user_stats));
    } else if (strcmp(pchVersion, "STEAMUSERSTATS_INTERFACE_VERSION008") == 0) {
        return reinterpret_cast<ISteamUserStats *>(static_cast<ISteamUserStats008 *>(steam_user_stats)); // Not found in public Archive, must be between 1.11-1.12
    } else if (strcmp(pchVersion, "STEAMUSERSTATS_INTERFACE_VERSION009") == 0) {
        return reinterpret_cast<ISteamUserStats *>(static_cast<ISteamUserStats009 *>(steam_user_stats));
    } else if (strcmp(pchVersion, "STEAMUSERSTATS_INTERFACE_VERSION010") == 0) {
        return reinterpret_cast<ISteamUserStats *>(static_cast<ISteamUserStats010 *>(steam_user_stats));
    } else if (strcmp(pchVersion, "STEAMUSERSTATS_INTERFACE_VERSION011") == 0) {
        return reinterpret_cast<ISteamUserStats *>(static_cast<ISteamUserStats011 *>(steam_user_stats));
    } else if (strcmp(pchVersion, "STEAMUSERSTATS_INTERFACE_VERSION012") == 0) {
        return reinterpret_cast<ISteamUserStats *>(static_cast<ISteamUserStats012 *>(steam_user_stats));
    } else if (strcmp(pchVersion, STEAMUSERSTATS_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamUserStats *>(static_cast<ISteamUserStats *>(steam_user_stats));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// returns the ISteamGameServerStats interface
ISteamGameServerStats *Steam_Client::GetISteamGameServerStats( HSteamUser hSteamuser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamuser) return NULL;

    if (strcmp(pchVersion, STEAMGAMESERVERSTATS_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamGameServerStats *>(static_cast<ISteamGameServerStats *>(steam_gameserverstats));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// returns apps interface
ISteamApps *Steam_Client::GetISteamApps( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;

    Steam_Apps *steam_apps_temp{};

    if (steam_pipes[hSteamPipe].type == Steam_Pipe_Type::SERVER) {
        steam_apps_temp = steam_gameserver_apps;
    } else {
        steam_apps_temp = steam_apps;
    }
    if ((strcmp(pchVersion, "STEAMAPPS_INTERFACE_VERSION001") == 0) || (strcmp(pchVersion, "SteamApps001") == 0)) {
        return reinterpret_cast<ISteamApps *>(static_cast<ISteamApps001 *>(steam_apps_temp));
    } else if (strcmp(pchVersion, "STEAMAPPS_INTERFACE_VERSION002") == 0) {
        return reinterpret_cast<ISteamApps *>(static_cast<ISteamApps002 *>(steam_apps_temp));
    } else if (strcmp(pchVersion, "STEAMAPPS_INTERFACE_VERSION003") == 0) {
        return reinterpret_cast<ISteamApps *>(static_cast<ISteamApps003 *>(steam_apps_temp));
    } else if (strcmp(pchVersion, "STEAMAPPS_INTERFACE_VERSION004") == 0) {
        return reinterpret_cast<ISteamApps *>(static_cast<ISteamApps004 *>(steam_apps_temp));
    } else if (strcmp(pchVersion, "STEAMAPPS_INTERFACE_VERSION005") == 0) {
        return reinterpret_cast<ISteamApps *>(static_cast<ISteamApps005 *>(steam_apps_temp));
    } else if (strcmp(pchVersion, "STEAMAPPS_INTERFACE_VERSION006") == 0) {
        return reinterpret_cast<ISteamApps *>(static_cast<ISteamApps006 *>(steam_apps_temp));
    } else if (strcmp(pchVersion, "STEAMAPPS_INTERFACE_VERSION007") == 0) {
        return reinterpret_cast<ISteamApps *>(static_cast<ISteamApps007 *>(steam_apps_temp));
    } else if (strcmp(pchVersion, "STEAMAPPS_INTERFACE_VERSION008") == 0) {
        return reinterpret_cast<ISteamApps*>(static_cast<ISteamApps008 *>(steam_apps_temp));
    } else if (strcmp(pchVersion, STEAMAPPS_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamApps *>(static_cast<ISteamApps *>(steam_apps_temp));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// networking
ISteamNetworking *Steam_Client::GetISteamNetworking( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;

    Steam_Networking *steam_networking_temp{};

    if (steam_pipes[hSteamPipe].type == Steam_Pipe_Type::SERVER) {
        steam_networking_temp = steam_gameserver_networking;
    } else {
        steam_networking_temp = steam_networking;
    }

    if (strcmp(pchVersion, "SteamNetworking001") == 0) {
        return reinterpret_cast<ISteamNetworking *>(static_cast<ISteamNetworking001 *>(steam_networking_temp));
    } else if (strcmp(pchVersion, "SteamNetworking002") == 0) {
        return reinterpret_cast<ISteamNetworking *>(static_cast<ISteamNetworking002 *>(steam_networking_temp));
    } else if (strcmp(pchVersion, "SteamNetworking003") == 0) {
        return reinterpret_cast<ISteamNetworking *>(static_cast<ISteamNetworking003 *>(steam_networking_temp));
    } else if (strcmp(pchVersion, "SteamNetworking004") == 0) {
        return reinterpret_cast<ISteamNetworking *>(static_cast<ISteamNetworking004 *>(steam_networking_temp));
    } else if (strcmp(pchVersion, "SteamNetworking005") == 0) {
        return reinterpret_cast<ISteamNetworking *>(static_cast<ISteamNetworking005 *>(steam_networking_temp));
    } else if (strcmp(pchVersion, STEAMNETWORKING_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamNetworking *>(static_cast<ISteamNetworking *>(steam_networking_temp));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// remote storage
ISteamRemoteStorage *Steam_Client::GetISteamRemoteStorage( HSteamUser hSteamuser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamuser) return NULL;

    if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION001") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage001 *>(steam_remote_storage)); //Not found in public Archive, must be before 1.00
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION002") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage002 *>(steam_remote_storage));
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION003") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage003 *>(steam_remote_storage)); //Not found in public Archive, must be between 1.11-1.12
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION004") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage004 *>(steam_remote_storage));
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION005") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage005 *>(steam_remote_storage));
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION006") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage006 *>(steam_remote_storage));
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION007") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage007 *>(steam_remote_storage)); //Not found in public Archive, must be between 1.19-1.20
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION008") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage008 *>(steam_remote_storage));
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION009") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage009 *>(steam_remote_storage)); //Not found in public Archive, must be between 1.21-1.22
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION010") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage010 *>(steam_remote_storage));
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION011") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage011 *>(steam_remote_storage));
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION012") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage012 *>(steam_remote_storage));
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION013") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage013 *>(steam_remote_storage));
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION014") == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage014 *>(steam_remote_storage));
    } else if (strcmp(pchVersion, "STEAMREMOTESTORAGE_INTERFACE_VERSION015") == 0) { // Not found in public Archive, based on reversing
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage015 *>(steam_remote_storage));
    } else if (strcmp(pchVersion, STEAMREMOTESTORAGE_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamRemoteStorage *>(static_cast<ISteamRemoteStorage *>(steam_remote_storage));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// user screenshots
ISteamScreenshots *Steam_Client::GetISteamScreenshots( HSteamUser hSteamuser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamuser) return NULL;

    if (strcmp(pchVersion, "STEAMSCREENSHOTS_INTERFACE_VERSION001") == 0) {
        return reinterpret_cast<ISteamScreenshots *>(static_cast<ISteamScreenshots001 *>(steam_screenshots));
    } else if (strcmp(pchVersion, "STEAMSCREENSHOTS_INTERFACE_VERSION002") == 0) {
        return reinterpret_cast<ISteamScreenshots *>(static_cast<ISteamScreenshots002 *>(steam_screenshots));
    } else if (strcmp(pchVersion, STEAMSCREENSHOTS_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamScreenshots *>(static_cast<ISteamScreenshots *>(steam_screenshots));
    }
    
    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}


// Expose HTTP interface
ISteamHTTP *Steam_Client::GetISteamHTTP( HSteamUser hSteamuser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamuser) return NULL;
    Steam_HTTP *steam_http_temp{};

    if (steam_pipes[hSteamPipe].type == Steam_Pipe_Type::SERVER) {
        steam_http_temp = steam_gameserver_http;
    } else {
        steam_http_temp = steam_http;
    }

    if (strcmp(pchVersion, "STEAMHTTP_INTERFACE_VERSION001") == 0) {
        return reinterpret_cast<ISteamHTTP *>(static_cast<ISteamHTTP001 *>(steam_http_temp));
    } else if (strcmp(pchVersion, "STEAMHTTP_INTERFACE_VERSION002") == 0) {
        return reinterpret_cast<ISteamHTTP *>(static_cast<ISteamHTTP002 *>(steam_http_temp));
    } else if (strcmp(pchVersion, STEAMHTTP_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamHTTP *>(static_cast<ISteamHTTP *>(steam_http_temp));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// Deprecated - the ISteamUnifiedMessages interface is no longer intended for public consumption.
void *Steam_Client::DEPRECATED_GetISteamUnifiedMessages( HSteamUser hSteamuser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamuser) return NULL;

    if (strcmp(pchVersion, STEAMUNIFIEDMESSAGES_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<void *>(static_cast<ISteamUnifiedMessages *>(steam_unified_messages));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

ISteamUnifiedMessages *Steam_Client::GetISteamUnifiedMessages( HSteamUser hSteamuser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamuser) return NULL;

    if (strcmp(pchVersion, STEAMUNIFIEDMESSAGES_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamUnifiedMessages *>(static_cast<ISteamUnifiedMessages *>(steam_unified_messages));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// Exposes the ISteamController interface
ISteamController *Steam_Client::GetISteamController( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;

    if (strcmp(pchVersion, "STEAMCONTROLLER_INTERFACE_VERSION") == 0) { // SDK <= 1.34
        return reinterpret_cast<ISteamController *>(static_cast<ISteamController001 *>(steam_controller));
    } else if (strcmp(pchVersion, "SteamController001") == 0) {
        return nullptr; // real steamclient64.dll returns null
    } else if (strcmp(pchVersion, "SteamController002") == 0) {
        return nullptr; // real steamclient64.dll returns null
    } else if (strcmp(pchVersion, "SteamController003") == 0) {
        return reinterpret_cast<ISteamController *>(static_cast<ISteamController003 *>(steam_controller));
    } else if (strcmp(pchVersion, "SteamController004") == 0) {
        return reinterpret_cast<ISteamController *>(static_cast<ISteamController004 *>(steam_controller));
    } else if (strcmp(pchVersion, "SteamController005") == 0) {
        return reinterpret_cast<ISteamController *>(static_cast<ISteamController005 *>(steam_controller));
    } else if (strcmp(pchVersion, "SteamController006") == 0) {
        return reinterpret_cast<ISteamController *>(static_cast<ISteamController006 *>(steam_controller));
    } else if (strcmp(pchVersion, "SteamController007") == 0) {
        return reinterpret_cast<ISteamController *>(static_cast<ISteamController007 *>(steam_controller));
    } else if (strcmp(pchVersion, STEAMCONTROLLER_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamController *>(static_cast<ISteamController *>(steam_controller));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// Exposes the ISteamUGC interface
ISteamUGC *Steam_Client::GetISteamUGC( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;
    Steam_UGC *steam_ugc_temp{};

    if (steam_pipes[hSteamPipe].type == Steam_Pipe_Type::SERVER) {
        steam_ugc_temp = steam_gameserver_ugc;
    } else {
        steam_ugc_temp = steam_ugc;
    }

    if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION") == 0) {
        //Is this actually a valid interface version?
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC001 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION001") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC001 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION002") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC002 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION003") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC003 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION004") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC004 *>(steam_ugc_temp)); // Not found in public Archive, must be between 1.32-1.33b
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION005") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC005 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION006") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC006 *>(steam_ugc_temp)); // Not found in public Archive, must be between 1.33b-1.34
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION007") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC007 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION008") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC008 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION009") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC009 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION010") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC010 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION011") == 0) { // Not found in public Archive, based on reversing
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC011 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION012") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC012 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION013") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC013 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION014") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC014 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION015") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC015 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION016") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC016 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION017") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC017 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION018") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC018 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION019") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC019 *>(steam_ugc_temp)); // not found in public sdk, based on reversing
    } else if (strcmp(pchVersion, "STEAMUGC_INTERFACE_VERSION020") == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC020 *>(steam_ugc_temp));
    } else if (strcmp(pchVersion, STEAMUGC_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamUGC *>(static_cast<ISteamUGC *>(steam_ugc_temp));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// returns app list interface, only available on specially registered apps
ISteamAppList *Steam_Client::GetISteamAppList( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;

    if (strcmp(pchVersion, STEAMAPPLIST_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamAppList *>(static_cast<ISteamAppList *>(steam_applist));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// Music Player
ISteamMusic *Steam_Client::GetISteamMusic( HSteamUser hSteamuser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamuser) return NULL;

    if (strcmp(pchVersion, STEAMMUSIC_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamMusic *>(static_cast<ISteamMusic *>(steam_music));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// Music Player Remote
ISteamMusicRemote *Steam_Client::GetISteamMusicRemote(HSteamUser hSteamuser, HSteamPipe hSteamPipe, const char *pchVersion)
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamuser) return NULL;

    if (strcmp(pchVersion, STEAMMUSICREMOTE_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamMusicRemote *>(static_cast<ISteamMusicRemote *>(steam_musicremote));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// html page display
ISteamHTMLSurface *Steam_Client::GetISteamHTMLSurface(HSteamUser hSteamuser, HSteamPipe hSteamPipe, const char *pchVersion)
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamuser) return NULL;

    if (strcmp(pchVersion, "STEAMHTMLSURFACE_INTERFACE_VERSION_001") == 0) {
        return reinterpret_cast<ISteamHTMLSurface *>(static_cast<ISteamHTMLSurface001 *>(steam_HTMLsurface)); // Not found in public Archive, must be before 1.31
    } else if (strcmp(pchVersion, "STEAMHTMLSURFACE_INTERFACE_VERSION_002") == 0) {
        return reinterpret_cast<ISteamHTMLSurface *>(static_cast<ISteamHTMLSurface002 *>(steam_HTMLsurface));
    } else if (strcmp(pchVersion, "STEAMHTMLSURFACE_INTERFACE_VERSION_003") == 0) {
        return reinterpret_cast<ISteamHTMLSurface *>(static_cast<ISteamHTMLSurface003 *>(steam_HTMLsurface));
    } else if (strcmp(pchVersion, "STEAMHTMLSURFACE_INTERFACE_VERSION_004") == 0) {
        return reinterpret_cast<ISteamHTMLSurface *>(static_cast<ISteamHTMLSurface004 *>(steam_HTMLsurface));
    } else if (strcmp(pchVersion, STEAMHTMLSURFACE_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamHTMLSurface *>(static_cast<ISteamHTMLSurface *>(steam_HTMLsurface));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// inventory
ISteamInventory *Steam_Client::GetISteamInventory( HSteamUser hSteamuser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamuser) return NULL;
    Steam_Inventory *steam_inventory_temp{};

    if (steam_pipes[hSteamPipe].type == Steam_Pipe_Type::SERVER) {
        steam_inventory_temp = steam_gameserver_inventory;
    } else {
        steam_inventory_temp = steam_inventory;
    }

    if (strcmp(pchVersion, "STEAMINVENTORY_INTERFACE_V001") == 0) {
        return reinterpret_cast<ISteamInventory *>(static_cast<ISteamInventory001 *>(steam_inventory_temp));
    } else if (strcmp(pchVersion, "STEAMINVENTORY_INTERFACE_V002") == 0) {
        return reinterpret_cast<ISteamInventory *>(static_cast<ISteamInventory002 *>(steam_inventory_temp));
    } else if (strcmp(pchVersion, STEAMINVENTORY_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamInventory *>(static_cast<ISteamInventory *>(steam_inventory_temp));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// Video
ISteamVideo *Steam_Client::GetISteamVideo( HSteamUser hSteamuser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamuser) return NULL;

    if (strcmp(pchVersion, "STEAMVIDEO_INTERFACE_V001") == 0) {
        return reinterpret_cast<ISteamVideo *>(static_cast<ISteamVideo001 *>(steam_video));
    }
    if (strcmp(pchVersion, "STEAMVIDEO_INTERFACE_V002") == 0) {
        return reinterpret_cast<ISteamVideo *>(static_cast<ISteamVideo002 *>(steam_video));
    }
    else if (strcmp(pchVersion, STEAMVIDEO_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamVideo *>(static_cast<ISteamVideo *>(steam_video));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// Parental controls
ISteamParentalSettings *Steam_Client::GetISteamParentalSettings( HSteamUser hSteamuser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamuser) return NULL;

    if (strcmp(pchVersion, STEAMPARENTALSETTINGS_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamParentalSettings *>(static_cast<ISteamParentalSettings *>(steam_parental));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

ISteamMasterServerUpdater *Steam_Client::GetISteamMasterServerUpdater( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;

    if (strcmp(pchVersion, STEAMMASTERSERVERUPDATER_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamMasterServerUpdater *>(static_cast<ISteamMasterServerUpdater *>(steam_masterserver_updater));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

ISteamContentServer *Steam_Client::GetISteamContentServer( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;
    return NULL;
}

// game search
ISteamGameSearch *Steam_Client::GetISteamGameSearch( HSteamUser hSteamuser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamuser) return NULL;

    if (strcmp(pchVersion, STEAMGAMESEARCH_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamGameSearch *>(static_cast<ISteamGameSearch *>(steam_game_search));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// Exposes the Steam Input interface for controller support
ISteamInput *Steam_Client::GetISteamInput( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;

    if (strcmp(pchVersion, "SteamInput001") == 0) {
        return reinterpret_cast<ISteamInput *>(static_cast<ISteamInput001 *>(steam_controller));
    } else if (strcmp(pchVersion, "SteamInput002") == 0) {
        return reinterpret_cast<ISteamInput *>(static_cast<ISteamInput002 *>(steam_controller));
    } else if (strcmp(pchVersion, "SteamInput005") == 0) {
        return reinterpret_cast<ISteamInput *>(static_cast<ISteamInput005 *>(steam_controller));
    } else if (strcmp(pchVersion, "SteamInput006") == 0) {
        return reinterpret_cast<ISteamInput *>(static_cast<ISteamInput006 *>(steam_controller));
    } else if (strcmp(pchVersion, STEAMINPUT_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamInput *>(static_cast<ISteamInput *>(steam_controller));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

// Steam Parties interface
ISteamParties *Steam_Client::GetISteamParties( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;

    if (strcmp(pchVersion, STEAMPARTIES_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamParties *>(static_cast<ISteamParties *>(steam_parties));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

ISteamRemotePlay *Steam_Client::GetISteamRemotePlay( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;

    if (strcmp(pchVersion, "STEAMREMOTEPLAY_INTERFACE_VERSION001") == 0) {
        return reinterpret_cast<ISteamRemotePlay *>(static_cast<ISteamRemotePlay001 *>(steam_remoteplay));
    } else if (strcmp(pchVersion, "STEAMREMOTEPLAY_INTERFACE_VERSION002") == 0) {
        return reinterpret_cast<ISteamRemotePlay *>(static_cast<ISteamRemotePlay002 *>(steam_remoteplay));
    } else if (strcmp(pchVersion, "STEAMREMOTEPLAY_INTERFACE_VERSION003") == 0) {
        return reinterpret_cast<ISteamRemotePlay*>(static_cast<ISteamRemotePlay003 *>(steam_remoteplay));
    } else if (strcmp(pchVersion, STEAMREMOTEPLAY_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamRemotePlay *>(static_cast<ISteamRemotePlay *>(steam_remoteplay));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

ISteamAppTicket *Steam_Client::GetAppTicket( HSteamUser hSteamUser, HSteamPipe hSteamPipe, const char *pchVersion )
{
    PRINT_DEBUG("%s", pchVersion);
    if (!steam_pipes.count(hSteamPipe) || !hSteamUser) return NULL;

    if (strcmp(pchVersion, STEAMAPPTICKET_INTERFACE_VERSION) == 0) {
        return reinterpret_cast<ISteamAppTicket *>(static_cast<ISteamAppTicket *>(steam_app_ticket));
    }

    return report_missing_impl_and_exit_or_null(pchVersion, EMU_FUNC_NAME);
}

void Steam_Client::report_missing_impl(std::string_view itf, std::string_view caller)
{
    PRINT_DEBUG("'%s' '%s'", itf.data(), caller.data());
    std::lock_guard lck(global_mutex);
    ++missing_interface_count;
    std::stringstream ss{};

    try {
        ss << "INTERFACE=" << itf << "\n";
        ss << "CALLER FN=" << caller << "\n";
    }
    catch(...) { }

#if defined(__WINDOWS__)
    // use a static variable as an address anchor in our DLL
    static const char emu_module_anchor = 0;

    // helper: replace user profile prefix with %USERPROFILE% to avoid leaking the username
    auto sanitize_path = [](const char* path) -> std::string {
        std::string result(path);
        char profile[MAX_PATH]{};
        if (GetEnvironmentVariableA("USERPROFILE", profile, MAX_PATH)) {
            size_t len = strlen(profile);
            if (len > 0 && result.size() >= len) {
                // case-insensitive prefix match
                if (_strnicmp(result.c_str(), profile, len) == 0) {
                    result.replace(0, len, "%USERPROFILE%");
                }
            }
        }
        return result;
    };

    // caller module detection via stack walk
    try {
        HMODULE our_module = nullptr;
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            reinterpret_cast<LPCWSTR>(&emu_module_anchor),
            &our_module
        );

        void* stack_frames[10]{};
        USHORT frame_count = CaptureStackBackTrace(0, 10, stack_frames, nullptr);
        for (USHORT i = 0; i < frame_count; ++i) {
            HMODULE frame_module = nullptr;
            if (GetModuleHandleExW(
                    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                    reinterpret_cast<LPCWSTR>(stack_frames[i]),
                    &frame_module) && frame_module && frame_module != our_module) {
                wchar_t module_path[MAX_PATH]{};
                if (GetModuleFileNameW(frame_module, module_path, MAX_PATH)) {
                    char module_path_a[MAX_PATH]{};
                    WideCharToMultiByte(CP_UTF8, 0, module_path, -1, module_path_a, MAX_PATH, nullptr, nullptr);
                    ss << "CALLER MODULE=" << sanitize_path(module_path_a) << "\n";
                    // return address offset within the calling module
                    auto offset = reinterpret_cast<uintptr_t>(stack_frames[i]) - reinterpret_cast<uintptr_t>(frame_module);
                    ss << "CALLER OFFSET=0x" << std::hex << offset << std::dec << "\n";
                }
                break;
            }
        }
    }
    catch(...) { }

    // process executable name
    try {
        wchar_t exe_path[MAX_PATH]{};
        if (GetModuleFileNameW(nullptr, exe_path, MAX_PATH)) {
            const wchar_t* exe_name = wcsrchr(exe_path, L'\\');
            exe_name = exe_name ? exe_name + 1 : exe_path;
            char exe_name_a[MAX_PATH]{};
            WideCharToMultiByte(CP_UTF8, 0, exe_name, -1, exe_name_a, MAX_PATH, nullptr, nullptr);
            ss << "PROCESS=" << exe_name_a << "\n";
        }
    }
    catch(...) { }

    // EMU DLL path
    try {
        HMODULE our_module = nullptr;
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            reinterpret_cast<LPCWSTR>(&emu_module_anchor),
            &our_module
        );
        if (our_module) {
            wchar_t emu_path[MAX_PATH]{};
            if (GetModuleFileNameW(our_module, emu_path, MAX_PATH)) {
                char emu_path_a[MAX_PATH]{};
                WideCharToMultiByte(CP_UTF8, 0, emu_path, -1, emu_path_a, MAX_PATH, nullptr, nullptr);
                ss << "EMU DLL=" << sanitize_path(emu_path_a) << "\n";
            }
        }
    }
    catch(...) { }

    // thread ID
    try {
        ss << "THREAD ID=" << GetCurrentThreadId() << "\n";
    }
    catch(...) { }

    // detected third-party modules (use cached results from detect_thirdparty_injectors)
    try {
        if (!overlays_scanned) {
            detect_thirdparty_injectors();
        }
        if (!cached_detected_overlays.empty()) {
            ss << "DETECTED OVERLAYS=" << cached_detected_overlays << "\n";
        }
    }
    catch(...) { }
#endif

    // injector detection status
    try {
        ss << "GRACEFUL=" << (thirdparty_injector_detected ? "true" : "false") << "\n";
    }
    catch(...) { }

    try {
        if (settings_client) {
            ss << "APPID=" << settings_client->get_local_game_id().AppID() << "\n";
        }
    }
    catch(...) { }

    // request counter
    try {
        ss << "REQUEST #=" << missing_interface_count << "\n";
    }
    catch(...) { }

    try {
        std::string time(common_helpers::get_utc_time());
        if (time.size()) {
           ss << "TIME=" << time << "\n";
        }

        ss << "--------------------\n" << std::endl;
    }
    catch(...) { }

    try {
        std::ofstream report(std::filesystem::u8path(get_full_program_path() + "EMU_MISSING_INTERFACE.txt"), std::ios::out | std::ios::app);
        if (report.is_open()) {
            report << ss.str();
        }
    }
    catch(...) { }

#if defined(__WINDOWS__)
    if (!thirdparty_injector_detected) {
        MessageBoxA(nullptr, ss.str().c_str(), "Missing interface", MB_OK);
    }
#endif
}

void Steam_Client::report_missing_impl_and_exit(std::string_view itf, std::string_view caller)
{
    report_missing_impl(itf, caller);
    std::exit(0x4155149); // MISSING :)
}

std::nullptr_t Steam_Client::report_missing_impl_and_exit_or_null(std::string_view itf, std::string_view caller)
{
    // re-check for late-injected third-party tools (e.g. Special K via global hook)
    if (!thirdparty_injector_detected) {
        detect_thirdparty_injectors();
    }

    // Special K caller: always graceful - SK probes interfaces from high versions down
    if (is_caller_special_k()) {
        PRINT_DEBUG("[GRACEFUL/SK] unknown interface '%s' requested by '%s', returning nullptr", itf.data(), caller.data());
        report_missing_impl(itf, caller);
        return nullptr;
    }

    // config: exit_on_unknown_interface (default true)
    // when false, return nullptr instead of crashing for any caller
    if (settings_client && !settings_client->exit_on_unknown_interface) {
        PRINT_DEBUG("[GRACEFUL/CFG] unknown interface '%s' requested by '%s', returning nullptr", itf.data(), caller.data());
        report_missing_impl(itf, caller);
        return nullptr;
    }

    // default: crash for debugging (non-SK, no config override)
    report_missing_impl_and_exit(itf, caller);
    // unreachable - report_missing_impl_and_exit is [[noreturn]]
    return nullptr;
}

void Steam_Client::detect_thirdparty_injectors()
{
    thirdparty_injector_detected = false;

#if defined(__WINDOWS__)
    // scan all known overlays/injectors and log each one found
    struct { const wchar_t* name; const char* label; const char* type; } known_modules[] = {
        // --- injectors / post-processors ---
        #if defined(_WIN64)
        { L"SpecialK64.dll",            "Special K",            "injector" },
        { L"ReShade64.dll",             "ReShade",              "post-processor" },
        #else
        { L"SpecialK32.dll",            "Special K",            "injector" },
        { L"ReShade32.dll",             "ReShade",              "post-processor" },
        #endif
        { L"d3dcompiler_46e.dll",       "ENB Series",           "post-processor" },

        // --- recording / streaming ---
        #if defined(_WIN64)
        { L"nvspcap64.dll",             "NVIDIA ShadowPlay",    "recording" },
        { L"graphics-hook64.dll",       "OBS Game Capture",     "recording" },
        { L"fraps64.dll",               "Fraps",                "recording" },
        { L"MedalHook64.dll",           "Medal.tv",             "recording" },
        { L"bdcam64.dll",               "Bandicam",             "recording" },
        { L"Action64.dll",              "Mirillis Action",      "recording" },
        { L"XSplit.Core64.dll",         "XSplit",               "recording" },
        { L"d3dgear64.dll",             "D3DGear",              "recording" },
        #else
        { L"nvspcap.dll",               "NVIDIA ShadowPlay",    "recording" },
        { L"graphics-hook32.dll",       "OBS Game Capture",     "recording" },
        { L"fraps32.dll",               "Fraps",                "recording" },
        { L"MedalHook.dll",             "Medal.tv",             "recording" },
        { L"bdcam32.dll",               "Bandicam",             "recording" },
        { L"Action.dll",                "Mirillis Action",      "recording" },
        { L"XSplit.Core.dll",           "XSplit",               "recording" },
        { L"d3dgear.dll",               "D3DGear",              "recording" },
        #endif
        { L"Streamlabs.dll",            "Streamlabs",           "recording" },

        // --- monitoring ---
        { L"RTSSHooks64.dll",           "RTSS",                 "monitoring" },
        { L"RTSSHooks.dll",             "RTSS",                 "monitoring" },
        #if defined(_WIN64)
        { L"fpshook64.dll",             "FPS Monitor",          "monitoring" },
        { L"PresentMon64.dll",          "Intel PresentMon",     "monitoring" },
        #else
        { L"fpshook.dll",               "FPS Monitor",          "monitoring" },
        { L"PresentMon32.dll",          "Intel PresentMon",     "monitoring" },
        #endif

        // --- store overlays ---
        { L"GameOverlayRenderer64.dll", "Steam Overlay",        "store overlay" },
        { L"GameOverlayRenderer.dll",   "Steam Overlay",        "store overlay" },
        { L"DiscordHook64.dll",         "Discord",              "store overlay" },
        { L"DiscordHook.dll",           "Discord",              "store overlay" },
        #if defined(_WIN64)
        { L"EOSOVH-Win64-Shipping.dll", "Epic Online Services", "store overlay" },
        { L"Galaxy64.dll",              "GOG Galaxy",           "store overlay" },
        { L"GalaxyOverlayRenderer64.dll", "GOG Galaxy",         "store overlay" },
        { L"igo64.dll",                 "EA App / Origin",      "store overlay" },
        { L"uplay_r2_loader64.dll",     "Ubisoft Connect",      "store overlay" },
        { L"upc_r2_loader64.dll",       "Ubisoft Connect",      "store overlay" },
        #else
        { L"EOSOVH-Win32-Shipping.dll", "Epic Online Services", "store overlay" },
        { L"Galaxy.dll",                "GOG Galaxy",           "store overlay" },
        { L"GalaxyOverlayRenderer.dll", "GOG Galaxy",           "store overlay" },
        { L"igo32.dll",                 "EA App / Origin",      "store overlay" },
        { L"uplay_r2_loader.dll",       "Ubisoft Connect",      "store overlay" },
        { L"upc_r2_loader.dll",         "Ubisoft Connect",      "store overlay" },
        #endif

        // --- GPU vendor software ---
        #if defined(_WIN64)
        { L"aaborern64.dll",            "AMD Adrenalin",        "gpu vendor" },
        { L"atiumd64.dll",              "AMD Display Driver",   "gpu vendor" },
        #else
        { L"aaborern.dll",              "AMD Adrenalin",        "gpu vendor" },
        { L"atiumdag.dll",              "AMD Display Driver",   "gpu vendor" },
        #endif
        { L"RadeonSoftware.dll",        "AMD Software",         "gpu vendor" },

        // --- system / platform overlays ---
        { L"GameBar.dll",               "Xbox Game Bar",        "system overlay" },
        { L"GameBarPresenceWriter.dll", "Xbox Game Bar",        "system overlay" },
        { L"SSOverlay64.dll",           "Samsung Gaming Hub",   "system overlay" },
        { L"SSOverlay.dll",             "Samsung Gaming Hub",   "system overlay" },
        { L"AcLayer.dll",               "Windows Compatibility","system overlay" },

        // --- gaming platforms / launchers ---
        { L"OWClient.dll",              "Overwolf",             "platform" },
        { L"OWExplorer.dll",            "Overwolf",             "platform" },
        #if defined(_WIN64)
        { L"ltc_game64.dll",            "Playnite",             "platform" },
        #else
        { L"ltc_game32.dll",            "Playnite",             "platform" },
        #endif

        // --- peripheral software ---
        #if defined(_WIN64)
        { L"Nahimic2OSD64.dll",         "Nahimic",              "peripheral" },
        { L"LogiOverlay64.dll",         "Logitech G Hub",       "peripheral" },
        { L"iCUEOverlay64.dll",         "Corsair iCUE",         "peripheral" },
        { L"SteelSeriesGG64.dll",       "SteelSeries GG",       "peripheral" },
        { L"RzChromaSDK64.dll",         "Razer Chroma",         "peripheral" },
        #else
        { L"Nahimic2OSD.dll",           "Nahimic",              "peripheral" },
        { L"LogiOverlay.dll",           "Logitech G Hub",       "peripheral" },
        { L"iCUEOverlay.dll",           "Corsair iCUE",         "peripheral" },
        { L"SteelSeriesGG.dll",         "SteelSeries GG",       "peripheral" },
        { L"RzChromaSDK.dll",           "Razer Chroma",         "peripheral" },
        #endif
        { L"NahimicOSD.dll",            "Nahimic",              "peripheral" },

        // --- communication ---
        #if defined(_WIN64)
        { L"mumble_ol_x64.dll",         "Mumble",               "communication" },
        { L"ts3overlay_hook_x64.dll",   "TeamSpeak",            "communication" },
        #else
        { L"mumble_ol.dll",             "Mumble",               "communication" },
        { L"ts3overlay_hook_x86.dll",   "TeamSpeak",            "communication" },
        #endif

        // --- VR ---
        { L"openvr_api.dll",            "SteamVR",              "vr" },
        #if defined(_WIN64)
        { L"vrclient_x64.dll",          "SteamVR Client",       "vr" },
        { L"LibOVRRT64_1.dll",          "Oculus Runtime",       "vr" },
        #else
        { L"vrclient.dll",              "SteamVR Client",       "vr" },
        { L"LibOVRRT32_1.dll",          "Oculus Runtime",       "vr" },
        #endif
        { L"OculusXRPlugin.dll",        "Oculus/Meta",          "vr" },

        // --- anti-cheat (informational) ---
        #if defined(_WIN64)
        { L"EasyAntiCheat_x64.dll",     "EasyAntiCheat",        "anti-cheat" },
        { L"BEService_x64.dll",         "BattlEye",             "anti-cheat" },
        { L"BEClient_x64.dll",          "BattlEye",             "anti-cheat" },
        #else
        { L"EasyAntiCheat_x86.dll",     "EasyAntiCheat",        "anti-cheat" },
        { L"BEService_x86.dll",         "BattlEye",             "anti-cheat" },
        { L"BEClient_x86.dll",          "BattlEye",             "anti-cheat" },
        #endif
        { L"easyanticheat.dll",         "EasyAntiCheat",        "anti-cheat" },
        { L"vanguard.dll",              "Vanguard",             "anti-cheat" },

        // --- modding / script hooks ---
        { L"ScriptHookV.dll",           "ScriptHookV",          "modding" },
        { L"ScriptHookRDR2.dll",        "ScriptHookRDR2",       "modding" },
        { L"ScriptHook.dll",            "ScriptHook",           "modding" },
        { L"ScriptHookDotNet.dll",      "ScriptHookDotNet",     "modding" },
        #if defined(_WIN64)
        { L"version.dll",               "ASI Loader",           "modding" },
        #else
        { L"version.dll",               "ASI Loader",           "modding" },
        #endif
    };
    std::set<std::string> seen;
    std::string detected;
    for (auto& entry : known_modules) {
        if (GetModuleHandleW(entry.name) && seen.insert(entry.label).second) {
            PRINT_DEBUG("detected [%s]: %s (via %ls)", entry.type, entry.label, entry.name);
            if (!detected.empty()) detected += ", ";
            detected += entry.label;
        }
    }

    // check proxy DLLs for Special K or ReShade exports
    const wchar_t* proxy_dlls[] = {
        L"dxgi.dll", L"d3d11.dll", L"d3d12.dll", L"d3d10_1.dll", L"d3d10.dll", L"d3d9.dll",
        L"d3d8.dll", L"ddraw.dll", L"dinput8.dll", L"dinput.dll", L"winmm.dll",
        L"OpenGL32.dll", L"version.dll", L"dsound.dll", L"wininet.dll", L"winhttp.dll",
        L"xinput1_1.dll", L"xinput1_2.dll", L"xinput1_3.dll", L"xinput1_4.dll",
        L"xinput9_1_0.dll", L"xinputuap.dll",
        L"binkw32.dll", L"bink2w32.dll", L"binkw64.dll", L"bink2w64.dll",
        L"vorbisFile.dll", L"msacm32.dll", L"msvfw32.dll", L"xlive.dll"
    };
    for (auto dll_name : proxy_dlls) {
        HMODULE hMod = GetModuleHandleW(dll_name);
        if (!hMod) continue;
        if (GetProcAddress(hMod, "SK_GetVersionStr")) {
            PRINT_DEBUG("detected Special K via proxy DLL '%ls'", dll_name);
            thirdparty_injector_detected = true;
            specialk_proxy_detected = true;
            if (seen.insert("Special K (proxy)").second) {
                if (!detected.empty()) detected += ", ";
                detected += "Special K (proxy)";
            }
        } else if (GetProcAddress(hMod, "ReShadeVersion")) {
            PRINT_DEBUG("detected ReShade via proxy DLL '%ls'", dll_name);
            reshade_proxy_detected = true;
            if (seen.insert("ReShade (proxy)").second) {
                if (!detected.empty()) detected += ", ";
                detected += "ReShade (proxy)";
            }
        } else if (GetProcAddress(hMod, "GetASILoadLibrary")) {
            PRINT_DEBUG("detected Ultimate ASI Loader via proxy DLL '%ls'", dll_name);
            if (seen.insert("Ultimate ASI Loader (proxy)").second) {
                if (!detected.empty()) detected += ", ";
                detected += "Ultimate ASI Loader (proxy)";
            }
        }
    }

    // Special K (global injection) sets the flag
    #if defined(_WIN64)
    if (GetModuleHandleW(L"SpecialK64.dll")) thirdparty_injector_detected = true;
    #else
    if (GetModuleHandleW(L"SpecialK32.dll")) thirdparty_injector_detected = true;
    #endif

    cached_detected_overlays = std::move(detected);
#endif

    overlays_scanned = true;
}

bool Steam_Client::is_caller_special_k()
{
#if defined(__WINDOWS__)
    static const char emu_anchor = 0;
    HMODULE our_module = nullptr;
    GetModuleHandleExW(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        reinterpret_cast<LPCWSTR>(&emu_anchor),
        &our_module
    );

    void* stack_frames[12]{};
    USHORT frame_count = CaptureStackBackTrace(0, 12, stack_frames, nullptr);
    for (USHORT i = 0; i < frame_count; ++i) {
        HMODULE frame_module = nullptr;
        if (GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                reinterpret_cast<LPCWSTR>(stack_frames[i]),
                &frame_module) && frame_module && frame_module != our_module) {
            // found the first external caller module - check if it's Special K
            // check global injection DLLs
            #if defined(_WIN64)
            HMODULE sk_global = GetModuleHandleW(L"SpecialK64.dll");
            #else
            HMODULE sk_global = GetModuleHandleW(L"SpecialK32.dll");
            #endif
            if (sk_global && frame_module == sk_global) return true;

            // check proxy DLLs with SK export
            if (GetProcAddress(frame_module, "SK_GetVersionStr")) return true;

            return false;
        }
    }
#endif
    return false;
}

void Steam_Client::try_start_specialk_injection()
{
#if defined(__WINDOWS__)
    if (!settings_client) return;

    // run detection first if not done yet
    if (!overlays_scanned) {
        detect_thirdparty_injectors();
    }

    // --- notification/banner disables (independent of auto_inject_specialk) ---

    // disable ReShade startup banner by writing to ReShade.ini in the game directory
    // takes effect on next launch (ReShade reads its config during DLL init before our code runs)
    if (settings_client->disable_reshade_banner && reshade_proxy_detected) {
        wchar_t game_dir[MAX_PATH]{};
        if (GetModuleFileNameW(nullptr, game_dir, MAX_PATH)) {
            wchar_t* last_sep = wcsrchr(game_dir, L'\\');
            if (last_sep) *(last_sep + 1) = L'\0';
            std::wstring reshade_ini = std::wstring(game_dir) + L"ReShade.ini";
            if (WritePrivateProfileStringW(L"OVERLAY", L"ShowStartupBanner", L"0", reshade_ini.c_str())) {
                PRINT_DEBUG("[NOTIFICATION] disabled ReShade startup banner in '%ls'", reshade_ini.c_str());
            }
        }
    }

    // disable Special K startup notification by writing Silent=true to the per-game SK profile
    // takes effect on next launch (SK reads its config during init before our code runs)
    if (settings_client->disable_specialk_notification) {
        // check if SK is present (proxy, global injection, or about to be auto-injected)
        bool sk_present = specialk_proxy_detected || settings_client->auto_inject_specialk;
        #if defined(_WIN64)
        if (!sk_present) sk_present = (GetModuleHandleW(L"SpecialK64.dll") != nullptr);
        #else
        if (!sk_present) sk_present = (GetModuleHandleW(L"SpecialK32.dll") != nullptr);
        #endif
        if (sk_present) {
            wchar_t exe_path_sn[MAX_PATH]{};
            if (GetModuleFileNameW(nullptr, exe_path_sn, MAX_PATH)) {
                const wchar_t* exe_name_sn = wcsrchr(exe_path_sn, L'\\');
                exe_name_sn = exe_name_sn ? exe_name_sn + 1 : exe_path_sn;

                std::wstring sk_root_sn;
                if (!settings_client->specialk_install_path.empty()) {
                    wchar_t tmp[MAX_PATH]{};
                    MultiByteToWideChar(CP_UTF8, 0, settings_client->specialk_install_path.c_str(), -1, tmp, MAX_PATH);
                    sk_root_sn = tmp;
                    auto sep = sk_root_sn.find_last_of(L"\\/");
                    if (sep != std::wstring::npos) {
                        std::wstring tail = sk_root_sn.substr(sep + 1);
                        for (auto& c : tail) c = towlower(c);
                        if (tail == L"skif.exe") sk_root_sn = sk_root_sn.substr(0, sep);
                    }
                }
                if (sk_root_sn.empty()) {
                    wchar_t local_appdata[MAX_PATH]{};
                    if (SHGetFolderPathW(nullptr, CSIDL_LOCAL_APPDATA, nullptr, 0, local_appdata) == S_OK) {
                        sk_root_sn = std::wstring(local_appdata) + L"\\Programs\\Special K";
                    }
                }
                if (!sk_root_sn.empty()) {
                    std::wstring ini_dir_sn = sk_root_sn + L"\\Profiles\\" + exe_name_sn;
                    std::wstring ini_path_sn = ini_dir_sn + L"\\SpecialK.ini";
                    // create with UTF-16LE BOM if it doesn't exist yet
                    if (GetFileAttributesW(ini_path_sn.c_str()) == INVALID_FILE_ATTRIBUTES) {
                        CreateDirectoryW(ini_dir_sn.c_str(), nullptr);
                        HANDLE hFile = CreateFileW(ini_path_sn.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
                        if (hFile != INVALID_HANDLE_VALUE) {
                            const unsigned char bom[] = { 0xFF, 0xFE };
                            DWORD written = 0;
                            WriteFile(hFile, bom, sizeof(bom), &written, nullptr);
                            CloseHandle(hFile);
                        }
                    }
                    if (WritePrivateProfileStringW(L"SpecialK.System", L"Silent", L"true", ini_path_sn.c_str())) {
                        PRINT_DEBUG("[NOTIFICATION] disabled SK startup notification in '%ls'", ini_path_sn.c_str());
                    }
                }
            }
        }
    }

    // --- auto-injection logic (requires auto_inject_specialk=1) ---
    if (!settings_client->auto_inject_specialk) return;

    // Special K is already loaded as a local proxy DLL — do not start SKIF (global injection would conflict)
    if (specialk_proxy_detected) {
        PRINT_DEBUG("[SK AUTO-INJECT] Special K detected as local proxy DLL, skipping SKIF global injection");
        // if SKIF is running, stop its injection service to avoid double-injection
        HANDLE hSnap0 = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if (hSnap0 != INVALID_HANDLE_VALUE) {
            wchar_t skif_path_buf[MAX_PATH]{};
            bool skif_running = false;
            PROCESSENTRY32W pe0{};
            pe0.dwSize = sizeof(pe0);
            if (Process32FirstW(hSnap0, &pe0)) {
                do {
                    if (_wcsicmp(pe0.szExeFile, L"SKIF.exe") == 0) {
                        HANDLE hProc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pe0.th32ProcessID);
                        if (hProc) {
                            DWORD path_len = MAX_PATH;
                            if (QueryFullProcessImageNameW(hProc, 0, skif_path_buf, &path_len)) {
                                skif_running = true;
                            }
                            CloseHandle(hProc);
                        }
                        break;
                    }
                } while (Process32NextW(hSnap0, &pe0));
            }
            CloseHandle(hSnap0);
            if (skif_running) {
                PRINT_DEBUG("[SK AUTO-INJECT] SKIF is running, sending Stop to prevent global injection conflict");
                SHELLEXECUTEINFOW sei_stop{};
                sei_stop.cbSize = sizeof(sei_stop);
                sei_stop.fMask = SEE_MASK_NOASYNC;
                sei_stop.lpFile = skif_path_buf;
                sei_stop.lpParameters = L"Stop";
                sei_stop.nShow = SW_HIDE;
                ShellExecuteExW(&sei_stop);
            }
        }

        // SK is already a local proxy — if ReShade is also a proxy, disable SK's ReShade plugin
        if (reshade_proxy_detected) {
            wchar_t exe_path[MAX_PATH]{};
            if (GetModuleFileNameW(nullptr, exe_path, MAX_PATH)) {
                const wchar_t* exe_name = wcsrchr(exe_path, L'\\');
                exe_name = exe_name ? exe_name + 1 : exe_path;

                // find SK install root from configured path or default location
                std::wstring sk_root;
                if (settings_client && !settings_client->specialk_install_path.empty()) {
                    wchar_t tmp[MAX_PATH]{};
                    MultiByteToWideChar(CP_UTF8, 0, settings_client->specialk_install_path.c_str(), -1, tmp, MAX_PATH);
                    sk_root = tmp;
                    // strip SKIF.exe if present
                    auto last_sep = sk_root.find_last_of(L"\\/");
                    if (last_sep != std::wstring::npos) {
                        std::wstring tail = sk_root.substr(last_sep + 1);
                        for (auto& c : tail) c = towlower(c);
                        if (tail == L"skif.exe") sk_root = sk_root.substr(0, last_sep);
                    }
                }
                if (sk_root.empty()) {
                    wchar_t local_appdata[MAX_PATH]{};
                    if (SHGetFolderPathW(nullptr, CSIDL_LOCAL_APPDATA, nullptr, 0, local_appdata) == S_OK) {
                        sk_root = std::wstring(local_appdata) + L"\\Programs\\Special K";
                    }
                }
                if (!sk_root.empty()) {
                    std::wstring ini_dir = sk_root + L"\\Profiles\\" + exe_name;
                    std::wstring ini_path = ini_dir + L"\\SpecialK.ini";
                    // only modify if the profile already exists — creating a minimal INI
                    // prevents SK from writing its defaults and breaks initialization
                    if (GetFileAttributesW(ini_path.c_str()) != INVALID_FILE_ATTRIBUTES) {
                        if (WritePrivateProfileStringW(L"SpecialK.Plugins", L"ReShade", L"false", ini_path.c_str())) {
                            PRINT_DEBUG("[SK AUTO-INJECT] disabled ReShade plugin in SK profile: '%ls'", ini_path.c_str());
                        } else {
                            PRINT_DEBUG("[SK AUTO-INJECT] failed to write SK profile INI (error %lu)", GetLastError());
                        }
                    } else {
                        PRINT_DEBUG("[SK AUTO-INJECT] SK profile not found at '%ls', skipping ReShade disable (SK will create it on first run)", ini_path.c_str());
                    }
                }
            }
        }

        return;
    }

    // ReShade is loaded as a local proxy — disable SK's ReShade plugin to prevent double-load
    // This must happen BEFORE the "SK already loaded" check, because if SK is already
    // globally injected, we'd return early and never reach the INI write below.
    if (reshade_proxy_detected) {
        PRINT_DEBUG("[SK AUTO-INJECT] ReShade detected as local proxy DLL, will disable SK ReShade plugin loading");
        wchar_t exe_path_rd[MAX_PATH]{};
        if (GetModuleFileNameW(nullptr, exe_path_rd, MAX_PATH)) {
            const wchar_t* exe_name_rd = wcsrchr(exe_path_rd, L'\\');
            exe_name_rd = exe_name_rd ? exe_name_rd + 1 : exe_path_rd;

            // try to find SK install root
            std::wstring sk_root_rd;
            if (settings_client && !settings_client->specialk_install_path.empty()) {
                wchar_t tmp[MAX_PATH]{};
                MultiByteToWideChar(CP_UTF8, 0, settings_client->specialk_install_path.c_str(), -1, tmp, MAX_PATH);
                sk_root_rd = tmp;
                auto last_sep = sk_root_rd.find_last_of(L"\\/");
                if (last_sep != std::wstring::npos) {
                    std::wstring tail = sk_root_rd.substr(last_sep + 1);
                    for (auto& c : tail) c = towlower(c);
                    if (tail == L"skif.exe") sk_root_rd = sk_root_rd.substr(0, last_sep);
                }
            }
            if (sk_root_rd.empty()) {
                wchar_t local_appdata[MAX_PATH]{};
                if (SHGetFolderPathW(nullptr, CSIDL_LOCAL_APPDATA, nullptr, 0, local_appdata) == S_OK) {
                    sk_root_rd = std::wstring(local_appdata) + L"\\Programs\\Special K";
                }
            }
            if (!sk_root_rd.empty()) {
                std::wstring ini_dir_rd = sk_root_rd + L"\\Profiles\\" + exe_name_rd;
                std::wstring ini_path_rd = ini_dir_rd + L"\\SpecialK.ini";
                // if the profile INI doesn't exist yet, create it with a UTF-16LE BOM
                // so WritePrivateProfileStringW writes in the same encoding SK uses
                if (GetFileAttributesW(ini_path_rd.c_str()) == INVALID_FILE_ATTRIBUTES) {
                    CreateDirectoryW(ini_dir_rd.c_str(), nullptr);
                    HANDLE hFile = CreateFileW(ini_path_rd.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
                    if (hFile != INVALID_HANDLE_VALUE) {
                        const unsigned char bom[] = { 0xFF, 0xFE };
                        DWORD written = 0;
                        WriteFile(hFile, bom, sizeof(bom), &written, nullptr);
                        CloseHandle(hFile);
                        PRINT_DEBUG("[SK AUTO-INJECT] created SK profile with UTF-16LE BOM: '%ls'", ini_path_rd.c_str());
                    }
                }
                if (WritePrivateProfileStringW(L"SpecialK.Plugins", L"ReShade", L"false", ini_path_rd.c_str())) {
                    PRINT_DEBUG("[SK AUTO-INJECT] disabled ReShade plugin in SK profile: '%ls'", ini_path_rd.c_str());
                } else {
                    PRINT_DEBUG("[SK AUTO-INJECT] failed to write SK profile INI (error %lu)", GetLastError());
                }
            }
        }
    }

    // check if Special K is already loaded
    #if defined(_WIN64)
    if (GetModuleHandleW(L"SpecialK64.dll")) {
        PRINT_DEBUG("[SK AUTO-INJECT] Special K already loaded, skipping");
        return;
    }
    #else
    if (GetModuleHandleW(L"SpecialK32.dll")) {
        PRINT_DEBUG("[SK AUTO-INJECT] Special K already loaded, skipping");
        return;
    }
    #endif

    // also check proxy DLLs for SK
    const wchar_t* proxy_dlls[] = {
        L"dxgi.dll", L"d3d11.dll", L"d3d12.dll", L"d3d10_1.dll", L"d3d10.dll", L"d3d9.dll",
        L"d3d8.dll", L"ddraw.dll", L"dinput8.dll", L"dinput.dll", L"winmm.dll",
        L"OpenGL32.dll", L"version.dll", L"dsound.dll", L"wininet.dll", L"winhttp.dll",
        L"xinput1_1.dll", L"xinput1_2.dll", L"xinput1_3.dll", L"xinput1_4.dll",
        L"xinput9_1_0.dll", L"xinputuap.dll",
        L"binkw32.dll", L"bink2w32.dll", L"binkw64.dll", L"bink2w64.dll",
        L"vorbisFile.dll", L"msacm32.dll", L"msvfw32.dll", L"xlive.dll"
    };
    for (auto dll_name : proxy_dlls) {
        HMODULE hMod = GetModuleHandleW(dll_name);
        if (hMod && GetProcAddress(hMod, "SK_GetVersionStr")) {
            PRINT_DEBUG("[SK AUTO-INJECT] Special K already loaded via proxy '%ls', skipping", dll_name);
            return;
        }
    }

    // find SKIF.exe in running processes and get its path
    HANDLE hSnap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    if (hSnap == INVALID_HANDLE_VALUE) {
        PRINT_DEBUG("[SK AUTO-INJECT] CreateToolhelp32Snapshot failed");
        return;
    }

    wchar_t skif_path[MAX_PATH]{};
    bool found_skif = false;

    PROCESSENTRY32W pe{};
    pe.dwSize = sizeof(pe);
    if (Process32FirstW(hSnap, &pe)) {
        do {
            if (_wcsicmp(pe.szExeFile, L"SKIF.exe") == 0) {
                // found SKIF process, get its full executable path
                HANDLE hProc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pe.th32ProcessID);
                if (hProc) {
                    DWORD path_len = MAX_PATH;
                    if (QueryFullProcessImageNameW(hProc, 0, skif_path, &path_len)) {
                        found_skif = true;
                    }
                    CloseHandle(hProc);
                }
                break;
            }
        } while (Process32NextW(hSnap, &pe));
    }
    CloseHandle(hSnap);

    if (!found_skif) {
        // SKIF not running — try to find and start it
        wchar_t skif_search_path[MAX_PATH]{};

        // check user-configured path first
        if (settings_client && !settings_client->specialk_install_path.empty()) {
            MultiByteToWideChar(CP_UTF8, 0, settings_client->specialk_install_path.c_str(), -1, skif_search_path, MAX_PATH);
            // append SKIF.exe if the path doesn't end with it
            std::wstring path_w(skif_search_path);
            if (path_w.size() >= 8) {
                std::wstring tail = path_w.substr(path_w.size() - 8);
                for (auto& c : tail) c = towlower(c);
                if (tail != L"skif.exe") {
                    if (path_w.back() != L'\\' && path_w.back() != L'/') path_w += L'\\';
                    path_w += L"SKIF.exe";
                }
            } else {
                if (!path_w.empty() && path_w.back() != L'\\' && path_w.back() != L'/') path_w += L'\\';
                path_w += L"SKIF.exe";
            }
            wcsncpy_s(skif_search_path, path_w.c_str(), MAX_PATH - 1);
        }

        // try default install location: %LOCALAPPDATA%/Programs/Special K/
        if (!skif_search_path[0]) {
            wchar_t local_appdata[MAX_PATH]{};
            if (SHGetFolderPathW(nullptr, CSIDL_LOCAL_APPDATA, nullptr, 0, local_appdata) == S_OK) {
                std::wstring default_path = std::wstring(local_appdata) + L"\\Programs\\Special K\\SKIF.exe";
                wcsncpy_s(skif_search_path, default_path.c_str(), MAX_PATH - 1);
            }
        }

        if (skif_search_path[0] && GetFileAttributesW(skif_search_path) != INVALID_FILE_ATTRIBUTES) {
            PRINT_DEBUG("[SK AUTO-INJECT] SKIF not running, starting from '%ls'", skif_search_path);
            
            // step 1: launch SKIF minimized (without Start Temp)
            SHELLEXECUTEINFOW sei_skif{};
            sei_skif.cbSize = sizeof(sei_skif);
            sei_skif.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
            sei_skif.lpFile = skif_search_path;
            sei_skif.lpParameters = L"Minimize";
            sei_skif.nShow = SW_HIDE;

            if (ShellExecuteExW(&sei_skif)) {
                if (sei_skif.hProcess) CloseHandle(sei_skif.hProcess);

                // step 2: wait for SKIF to initialize (poll for process)
                PRINT_DEBUG("[SK AUTO-INJECT] waiting for SKIF to initialize...");
                const int init_timeout_ms = 5000;
                const int init_poll_ms = 250;
                int init_elapsed = 0;
                bool skif_ready = false;

                while (init_elapsed < init_timeout_ms) {
                    Sleep(init_poll_ms);
                    init_elapsed += init_poll_ms;
                    // check if SKIF created its window (FindWindow with SKIF's class)
                    // or just check if the process is running and responsive
                    HANDLE hSnap2 = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
                    if (hSnap2 != INVALID_HANDLE_VALUE) {
                        PROCESSENTRY32W pe2{};
                        pe2.dwSize = sizeof(pe2);
                        if (Process32FirstW(hSnap2, &pe2)) {
                            do {
                                if (_wcsicmp(pe2.szExeFile, L"SKIF.exe") == 0) {
                                    skif_ready = true;
                                    break;
                                }
                            } while (Process32NextW(hSnap2, &pe2));
                        }
                        CloseHandle(hSnap2);
                    }
                    if (skif_ready) break;
                }

                if (skif_ready) {
                    // give SKIF a bit more time to fully initialize its D3D11 renderer
                    Sleep(2000);
                    found_skif = true;
                    wcsncpy_s(skif_path, skif_search_path, MAX_PATH - 1);
                    PRINT_DEBUG("[SK AUTO-INJECT] SKIF started and ready after %d ms", init_elapsed + 2000);
                } else {
                    PRINT_DEBUG("[SK AUTO-INJECT] SKIF process did not appear within %d ms", init_timeout_ms);
                }
            } else {
                PRINT_DEBUG("[SK AUTO-INJECT] failed to launch SKIF (error %lu)", GetLastError());
            }
        } else {
            PRINT_DEBUG("[SK AUTO-INJECT] SKIF not found at '%ls'", skif_search_path[0] ? skif_search_path : L"(no path)");
        }
    }

    if (!found_skif) {
        PRINT_DEBUG("[SK AUTO-INJECT] SKIF.exe not found, skipping auto-injection");
        return;
    }

    // if ReShade is loaded as a local proxy, disable SK's ReShade plugin loading
    // by writing ReShade=false to the per-game SK profile before triggering injection
    if (reshade_proxy_detected) {
        wchar_t exe_path[MAX_PATH]{};
        if (GetModuleFileNameW(nullptr, exe_path, MAX_PATH)) {
            const wchar_t* exe_name = wcsrchr(exe_path, L'\\');
            exe_name = exe_name ? exe_name + 1 : exe_path;

            // determine SK install root from SKIF path (strip SKIF.exe)
            std::wstring sk_root(skif_path);
            auto last_sep = sk_root.find_last_of(L"\\/");
            if (last_sep != std::wstring::npos) {
                sk_root = sk_root.substr(0, last_sep);
            }

            // build profile INI path: <SK root>/Profiles/<game.exe>/SpecialK.ini
            std::wstring ini_dir = sk_root + L"\\Profiles\\" + exe_name;
            std::wstring ini_path = ini_dir + L"\\SpecialK.ini";

            // if the profile INI doesn't exist yet, create it with a UTF-16LE BOM
            // so WritePrivateProfileStringW writes in the same encoding SK uses
            if (GetFileAttributesW(ini_path.c_str()) == INVALID_FILE_ATTRIBUTES) {
                CreateDirectoryW(ini_dir.c_str(), nullptr);
                HANDLE hFile = CreateFileW(ini_path.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
                if (hFile != INVALID_HANDLE_VALUE) {
                    const unsigned char bom[] = { 0xFF, 0xFE };
                    DWORD written = 0;
                    WriteFile(hFile, bom, sizeof(bom), &written, nullptr);
                    CloseHandle(hFile);
                    PRINT_DEBUG("[SK AUTO-INJECT] created SK profile with UTF-16LE BOM: '%ls'", ini_path.c_str());
                }
            }
            if (WritePrivateProfileStringW(L"SpecialK.Plugins", L"ReShade", L"false", ini_path.c_str())) {
                PRINT_DEBUG("[SK AUTO-INJECT] disabled ReShade plugin in SK profile: '%ls'", ini_path.c_str());
            } else {
                PRINT_DEBUG("[SK AUTO-INJECT] failed to write SK profile INI (error %lu)", GetLastError());
            }
        }
    }

    // check if the SK injection service is already running
    bool service_already_running = false;
    {
        HANDLE hSnap2 = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if (hSnap2 != INVALID_HANDLE_VALUE) {
            PROCESSENTRY32W pe2{};
            pe2.dwSize = sizeof(pe2);
            if (Process32FirstW(hSnap2, &pe2)) {
                do {
                    if (_wcsicmp(pe2.szExeFile, L"SKIFsvc64.exe") == 0 ||
                        _wcsicmp(pe2.szExeFile, L"SKIFsvc32.exe") == 0) {
                        service_already_running = true;
                        break;
                    }
                } while (Process32NextW(hSnap2, &pe2));
            }
            CloseHandle(hSnap2);
        }
    }

    if (service_already_running) {
        PRINT_DEBUG("[SK AUTO-INJECT] SK injection service already running, skipping service start");
    } else {
        // determine service mode based on specialk_service_duration
        unsigned duration = settings_client ? settings_client->specialk_service_duration : 0;
        const wchar_t* params = duration > 0 ? L"Start" : L"Start Temp";

        PRINT_DEBUG("[SK AUTO-INJECT] sending '%ls' to SKIF at '%ls'...", params, skif_path);

        SHELLEXECUTEINFOW sei{};
        sei.cbSize = sizeof(sei);
        sei.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        sei.lpFile = skif_path;
        sei.lpParameters = params;
        sei.nShow = SW_HIDE;

        if (!ShellExecuteExW(&sei)) {
            PRINT_DEBUG("[SK AUTO-INJECT] failed to launch SKIF injection service (error %lu)", GetLastError());
            return;
        }

        if (sei.hProcess) {
            CloseHandle(sei.hProcess);
        }

        // if using timed mode, schedule a background thread to stop the service
        if (duration > 0) {
            std::wstring stop_path(skif_path);
            std::thread([stop_path, duration]() {
                Sleep(duration * 1000);
                SHELLEXECUTEINFOW sei_stop{};
                sei_stop.cbSize = sizeof(sei_stop);
                sei_stop.fMask = SEE_MASK_NOASYNC;
                sei_stop.lpFile = stop_path.c_str();
                sei_stop.lpParameters = L"Stop";
                sei_stop.nShow = SW_HIDE;
                ShellExecuteExW(&sei_stop);
                PRINT_DEBUG("[SK AUTO-INJECT] stopped injection service after %u seconds", duration);
            }).detach();
            PRINT_DEBUG("[SK AUTO-INJECT] service will auto-stop in %u seconds", duration);
        }
    }

    // wait for Special K DLL to appear in our process (timeout: 10 seconds)
    PRINT_DEBUG("[SK AUTO-INJECT] waiting for Special K to inject...");
    const int timeout_ms = 10000;
    const int poll_ms = 100;
    int elapsed = 0;
    bool injected = false;

    while (elapsed < timeout_ms) {
        #if defined(_WIN64)
        if (GetModuleHandleW(L"SpecialK64.dll")) { injected = true; break; }
        #else
        if (GetModuleHandleW(L"SpecialK32.dll")) { injected = true; break; }
        #endif

        // also check proxy DLLs
        for (auto dll_name : proxy_dlls) {
            HMODULE hMod = GetModuleHandleW(dll_name);
            if (hMod && GetProcAddress(hMod, "SK_GetVersionStr")) {
                injected = true;
                break;
            }
        }
        if (injected) break;

        Sleep(poll_ms);
        elapsed += poll_ms;
    }

    if (injected) {
        PRINT_DEBUG("[SK AUTO-INJECT] Special K successfully injected after %d ms", elapsed);
        thirdparty_injector_detected = true;
    } else {
        PRINT_DEBUG("[SK AUTO-INJECT] timed out waiting for Special K injection (%d ms)", timeout_ms);
    }
#endif
}
