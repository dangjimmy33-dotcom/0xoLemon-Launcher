#pragma once
#include <cstddef>
#include <cstdint>

// Inbound CM packet observer (mirror of Windows RecvPktMonitorHook): pass-through
// detour on CCMInterface::RecvPkt that hands EMsg 819 to SchemaFetch::HandleInbound819.

namespace RecvPktHook {

// Resolve CCMInterface::RecvPkt and install the observer detour; returns true on success.
bool Install(uintptr_t steamclientBase, size_t steamclientSize);

// Remove the detour and wait for in-flight observers to drain.
void Remove();

} // namespace RecvPktHook
