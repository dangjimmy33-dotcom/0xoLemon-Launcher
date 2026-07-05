#pragma once
#include <cstdint>
#include <cstddef>

// Linux schema fetch: sends CMsgClientGetUserStats (818) for a discovered owner's
// SteamID, captures the 819 reply, and writes UserGameStatsSchema_<appid>.bin.
namespace SchemaFetch {

// Matches the existing ParseFromArray_t in cloud_hooks.cpp / LivePlaytime.
using ParseFromArrayFn = int(*)(void* msg, const void* data, int len);

// Resolve steamclient.so entry points; called once during init.
bool Resolve(uintptr_t steamclientBase, size_t steamclientSize,
             ParseFromArrayFn parseFromArray);

// Capture connHandle + session fields (steamid, session_id, realm) from a live
// outbound CM message; needed by our injected 818 requests.
void CaptureFromOutbound(uint32_t emsg, void* msgObj, void* cmInterface);

// Drain queued schema-fetch sends on the network thread.
void DrainOnNetThread();

// Handle an inbound 819: correlate by game_id (appid) and, if it carries a schema,
// write the .bin + stats template. Returns true if a matching reply was consumed.
bool HandleInbound819(const uint8_t* data, uint32_t len);

// Signal shutdown to abort pending HTTP work and stop sending.
void Shutdown();

} // namespace SchemaFetch
