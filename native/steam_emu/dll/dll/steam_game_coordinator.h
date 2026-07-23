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

#ifndef __INCLUDED_STEAM_GAME_COORDINATOR_H__
#define __INCLUDED_STEAM_GAME_COORDINATOR_H__

#include "base.h"
#include "econ_item.h"

class Steam_User_Items;
class Steam_GameServer_Items;
struct GCMsgHdr_t;
struct GCMsgHdrEx_t;
struct ProtoBufMsgHeader_t;
class CMsgProtoBufHeader;

class Steam_Game_Coordinator :
public ISteamGameCoordinator
{
    static constexpr const auto items_user_file = "items.json";
    static constexpr const auto gc_config_file = "gc.json";
    constexpr const static uint32 protobuf_mask = 0x80000000;

    class Settings *settings{};
    class Networking *network{};
    class Local_Storage *local_storage{};
    class SteamCallBacks *callbacks{};
    class RunEveryRunCB *run_every_runcb{};
    bool is_server{};

    struct GC_Message
    {
        uint32 msg_type{};
        std::string msg_body;
        std::chrono::high_resolution_clock::time_point created{};
        double post_in{};
    };

    std::vector<GC_Message> pending_messages;
    std::queue<GC_Message> incoming_messages;

    enum GC_Profile
    {
        GC_PROFILE_INVALID = 0,
        GC_PROFILE_TF2,
        //GC_PROFILE_PORTAL2,
    };

    int gc_version{};
    GC_Profile gc_profile{};
    bool is_portal2{};
    bool gc_initialized{};
    bool delay_init{};
    bool welcome_received{};
    std::chrono::high_resolution_clock::time_point welcome_time{};

    std::vector<Econ_Item> items;
    bool items_loaded{};

    struct RequestInventory
    {
        std::chrono::high_resolution_clock::time_point created{};
        CSteamID steam_id{};
        SteamAPICall_t steam_api_call{};
        bool is_gc{};
    };

    std::map<CSteamID, std::vector<Econ_Item>> all_user_items;
    std::vector<RequestInventory> pending_items_requests;

    bool gc_enabled();
    Steam_User_Items *client_items();
    Steam_GameServer_Items *server_items();
    void parse_gc_config();
    bool is_welcome_message(const GC_Message &message);
    void push_incoming(uint32 msg_type, const std::string &message, double delay = 0.1);

    std::string build_msg_header(JobID_t target_job = k_GIDNil, JobID_t source_job = k_GIDNil);
    GCMsgHdrEx_t parse_msg_header(const char *&p);
    std::string build_protomsg_header(uint32 msg_type, JobID_t target_job = k_GIDNil, JobID_t source_job = k_GIDNil);
    template <class T>
    std::tuple<ProtoBufMsgHeader_t, CMsgProtoBufHeader, T, bool> parse_protomsg(const void *input, uint32 input_size);
    uint64 item_id_local_to_network(uint64 item_id);
    uint64 item_id_network_to_local(uint64 item_id);
    std::string item_to_gcstruct(const Econ_Item &item, CSteamID steam_id);
    std::string item_to_gcprotobuf(const Econ_Item &item, CSteamID steam_id);

    void handle_set_item_pos(const void *input, uint32 input_size);
    void handle_delete_item(const void *input, uint32 input_size);
    void handle_motd_request(const void *input, uint32 input_size);
    void handle_respawn(const void *input, uint32 input_size);
    void handle_set_item_style(const void *input, uint32 input_size);
    void handle_adjust_equip_state(const void *input, uint32 input_size);
    void handle_set_multiple_item_pos(const void *input, uint32 input_size);

    void callback_client_welcome();
    void callback_server_welcome();
    void callback_items_received(CSteamID steam_id, const std::vector<Econ_Item> &items);
    void callback_items_removed(CSteamID steam_id);
    void callback_item_updated(CSteamID steam_id, const Econ_Item &item);
    void callback_item_deleted(CSteamID steam_id, uint64 item_id);
    void callback_respawn_request(CSteamID steam_id);

    void network_callback_inventory_request(Common_Message *msg);
    void network_callback_inventory_response(Common_Message *msg);
    void network_callback_item_update(Common_Message *msg);
    void network_callback_item_deletion(Common_Message *msg);
    void network_callback_respawn_request(Common_Message *msg);
    void network_callback(Common_Message *msg);
    void RunCallbacks();

    static void steam_network_callback(void *object, Common_Message *msg);
    static void steam_run_every_runcb(void *object);

public:
    Steam_Game_Coordinator(class Settings *settings, class Networking *network, class Local_Storage *local_storage, class SteamCallBacks *callbacks, class RunEveryRunCB *run_every_runcb, bool is_server);
    ~Steam_Game_Coordinator();

    void initialize_gc();
    void shutdown_gc();

    const std::vector<Econ_Item> &get_items() { return items; }
    const std::map<CSteamID, std::vector<Econ_Item>> &get_all_user_items() { return all_user_items; }
    const bool has_items_for_user(CSteamID steam_id) { return (all_user_items.count(steam_id) != 0); }
    const std::vector<Econ_Item> &get_items_for_user(CSteamID steam_id) { return all_user_items.at(steam_id); }

    const std::vector<Econ_Item> &load_items_from_file();
    void save_items_to_file();
    const Econ_Item *set_item_pos(uint64 item_id, uint32 inv_pos, bool is_gc, bool save = true);
    bool delete_item(uint64 item_id, bool is_gc);

    void request_user_items(CSteamID steam_id, SteamAPICall_t api_call, bool is_gc);
    SteamAPICall_t find_items_request(CSteamID steam_id);
    void remove_user_items(CSteamID steam_id);

    void on_client_connected(CSteamID steam_id);
    void on_client_disconnected(CSteamID steam_id);

    // sends a message to the Game Coordinator
    EGCResults SendMessage_( uint32 unMsgType, const void *pubData, uint32 cubData );

    // returns true if there is a message waiting from the game coordinator
    bool IsMessageAvailable( uint32 *pcubMsgSize );

    // fills the provided buffer with the first message in the queue and returns k_EGCResultOK or 
    // returns k_EGCResultNoMessage if there is no message waiting. pcubMsgSize is filled with the message size.
    // If the provided buffer is not large enough to fit the entire message, k_EGCResultBufferTooSmall is returned
    // and the message remains at the head of the queue.
    EGCResults RetrieveMessage( uint32 *punMsgType, void *pubDest, uint32 cubDest, uint32 *pcubMsgSize );

};

#endif // __INCLUDED_STEAM_GAME_COORDINATOR_H__
