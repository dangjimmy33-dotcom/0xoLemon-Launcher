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

#ifndef __INCLUDED_CAPICMCALLBACK_H__
#define __INCLUDED_CAPICMCALLBACK_H__

#include "base.h"

typedef void (*OnLogonSuccessFunc)();
typedef void (*OnLogonFailureFunc)(EResult);
typedef void (*OnLoggedOffFunc)(EResult);
typedef void (*OnBeginLogonRetryFunc)();
typedef void (*GSHandleClientApproveFunc)(uint64);
typedef void (*GSHandleClientDenyFunc)(uint64, EDenyReason);
typedef void (*GSHandleClientKickFunc)(uint64, EDenyReason);
typedef int (*Steam2GetValueFunc)(const char *, char *, int);

class CAPI_CMCallback : public ICMCallback, public ISteam2Auth
{
    OnLogonSuccessFunc OnLogonSuccess_ptr{};
    OnLogonFailureFunc OnLogonFailure_ptr{};
    OnLoggedOffFunc OnLoggedOff_ptr{};
    OnBeginLogonRetryFunc OnBeginLogonRetry_ptr{};
    GSHandleClientApproveFunc GSHandleClientApprove_ptr{};
    GSHandleClientDenyFunc GSHandleClientDeny_ptr{};
    GSHandleClientKickFunc GSHandleClientKick_ptr{};
    Steam2GetValueFunc GetValue_ptr{};

public:
    CAPI_CMCallback(OnLogonSuccessFunc func1,
        OnLogonFailureFunc func2,
        OnLoggedOffFunc func3,
        OnBeginLogonRetryFunc func4,
        GSHandleClientApproveFunc func5,
        GSHandleClientDenyFunc func6,
        GSHandleClientKickFunc func7,
        Steam2GetValueFunc func8)
    {
        OnLogonSuccess_ptr = func1;
        OnLogonFailure_ptr = func2;
        OnLoggedOff_ptr = func3;
        OnBeginLogonRetry_ptr = func4;
        GSHandleClientApprove_ptr = func5;
        GSHandleClientDeny_ptr = func6;
        GSHandleClientKick_ptr = func7;
        GetValue_ptr = func8;
    }
    ~CAPI_CMCallback() {}

    void OnLogonSuccess() { OnLogonSuccess_ptr(); }
    void OnLogonFailure(EResult eResult) { OnLogonFailure_ptr(eResult); }
    void OnLoggedOff(EResult eResult) { OnLoggedOff_ptr(eResult); }
    void OnBeginLogonRetry() { OnBeginLogonRetry_ptr(); }
    void HandleVACChallenge(int nClientGameID, uint8 *pubChallenge, int cubChallenge) {}
    void GSHandleClientApprove(CSteamID &steamID) { GSHandleClientApprove_ptr(steamID.ConvertToUint64()); }
    void GSHandleClientDeny(CSteamID &steamID, EDenyReason eDenyReason) { GSHandleClientDeny_ptr(steamID.ConvertToUint64(), eDenyReason); }
    void GSHandleClientKick(CSteamID &steamID, EDenyReason eDenyReason) { GSHandleClientKick_ptr(steamID.ConvertToUint64(), eDenyReason); }

    int GetValue(const char *var, char *buf, int bufsize) { return GetValue_ptr(var, buf, bufsize); }
    int GetServerReadableTicket(uint32 unk1, uint32 unk2, void *unk3, uint32 unk4, uint32 *unk5) { return 0; }
};

#endif // __INCLUDED_CAPICMCALLBACK_H__
