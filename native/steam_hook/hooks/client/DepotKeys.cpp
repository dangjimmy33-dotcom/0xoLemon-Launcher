// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "hooks/client/DepotKeys.h"
#include "hooks/Macros.h"
#include "core/entry.h"
#include <cstring>

namespace {
    LM_HOOK(LoadDepotDecryptionKey, int32, void* pObject, uint32 foo,char* KeyName, char* Key, uint32 KeySize) {
        // scan for depot key marker using raw pointers instead of string views
        const char* tagEnd = strstr(KeyName, "\\DecryptionKey");
        if (!tagEnd)
            return oLoadDepotDecryptionKey(pObject, foo, KeyName, Key, KeySize);

        const char* segStart = tagEnd;
        while (segStart > KeyName && segStart[-1] != '\\')
            --segStart;
        if (segStart <= KeyName)
            return oLoadDepotDecryptionKey(pObject, foo, KeyName, Key, KeySize);

        char* end = nullptr;
        AppId_t depotId = static_cast<AppId_t>(strtoul(segStart, &end, 10));
        if (end != tagEnd || depotId == 0)
            return oLoadDepotDecryptionKey(pObject, foo, KeyName, Key, KeySize);

        const auto& keyBlob = LuaLoader::GetDecryptionKey(depotId);
        if (keyBlob.empty())
            return oLoadDepotDecryptionKey(pObject, foo, KeyName, Key, KeySize);

        if (keyBlob.size() > KeySize) {
            LOG_DECRYPTIONKEYCH_WARN("depot {} key too fat ({} > {})", depotId, keyBlob.size(), KeySize);
            return oLoadDepotDecryptionKey(pObject, foo, KeyName, Key, KeySize);
        }

        memcpy(Key, keyBlob.data(), keyBlob.size());
        LOG_DECRYPTIONKEYCH_INFO("slapped depot key {} ({} bytes)", depotId, keyBlob.size());
        return static_cast<int32>(keyBlob.size());
    }
}

namespace DepotKeys {
    void Install() {
        LM_TX_BEGIN();
        LM_INSTALL(LoadDepotDecryptionKey);
        LM_TX_COMMIT();
    }

    void Uninstall() {
        LM_TX_BEGIN();
        LM_REMOVE(LoadDepotDecryptionKey);
        LM_TX_COMMIT();
    }
}
