#include "metadata_sync.h"

namespace MetadataSync {

std::atomic<bool> steamToolsPresent{false};
std::atomic<bool> syncLuas{false};
// Default OFF: WIP opt-in features; the user enables them.
std::atomic<bool> syncAchievements{false};
std::atomic<bool> syncPlaytime{false};
// Default ON: fetch missing schemas from CM when StGateOpen().
std::atomic<bool> schemaFetch{true};

}
