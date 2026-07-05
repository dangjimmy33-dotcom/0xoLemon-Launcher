#pragma once
#include <cstddef>
#include <cstdint>

// Taps CCMInterface::Send to track playtime sessions (CMsgClientGamesPlayed) and to
// block outbound 818s for namespace apps so our injected 819 is the sole response.

namespace GamesPlayedHook {

// Serialize a protobuf body to raw bytes (thread-local buffer, valid until next call).
using SerializeBodyFn = const uint8_t* (*)(void* bodyObj, size_t* outLen);
void SetSerializer(SerializeBodyFn fn);

// Resolve CCMInterface::Send and install the observer detour; returns true on success.
bool Install(uintptr_t steamclientBase, size_t steamclientSize);

// Remove the detour and wait for in-flight observers to drain.
void Remove();

// Resolved CCMInterface::Send entry (the real wire-send), valid after Install().
// Returns nullptr if the signature was not found. Other modules reuse this rather
// than re-scanning, because the inline detour overwrites the post-prologue bytes
// that any signature would need to match.
void* GetSendFunc();

} // namespace GamesPlayedHook
