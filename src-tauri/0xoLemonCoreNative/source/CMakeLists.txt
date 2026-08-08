cmake_minimum_required(VERSION 3.20)
project(OxoLemon VERSION 1.0.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_C_STANDARD 11)

if(POLICY CMP0091)
    cmake_policy(SET CMP0091 NEW)
endif()

set(CMAKE_CONFIGURATION_TYPES "Debug;Release" CACHE STRING "" FORCE)
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)

list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/cmake")
include(LcLua)
include(LcDetours)
include(LcSpdlog)
include(LcProtobuf)
include(LcTomlplusplus)
include(LcLogMacros)

set(PROTO_SRC "${CMAKE_CURRENT_SOURCE_DIR}/proto/steam_messages.proto")
set(PROTO_GEN_DIR       "${CMAKE_CURRENT_BINARY_DIR}/proto")
set(PROTO_GEN_LITE_DIR  "${CMAKE_CURRENT_BINARY_DIR}/proto_lite")

add_custom_command(
    OUTPUT "${PROTO_GEN_DIR}/steam_messages.pb.cc"
           "${PROTO_GEN_DIR}/steam_messages.pb.h"
    COMMAND ${CMAKE_COMMAND} -E make_directory "${PROTO_GEN_DIR}"
    COMMAND $<TARGET_FILE:protoc>
        "--cpp_out=${PROTO_GEN_DIR}"
        "-I${CMAKE_CURRENT_SOURCE_DIR}/proto"
        "${PROTO_SRC}"
    DEPENDS "${PROTO_SRC}" protoc
    COMMENT "Generating protobuf full-Message sources (Debug)"
)

add_custom_command(
    OUTPUT "${PROTO_GEN_LITE_DIR}/steam_messages.pb.cc"
           "${PROTO_GEN_LITE_DIR}/steam_messages.pb.h"
    COMMAND ${CMAKE_COMMAND} -E make_directory "${PROTO_GEN_LITE_DIR}"
    COMMAND $<TARGET_FILE:protoc>
        "--cpp_out=lite:${PROTO_GEN_LITE_DIR}"
        "-I${CMAKE_CURRENT_SOURCE_DIR}/proto"
        "${PROTO_SRC}"
    DEPENDS "${PROTO_SRC}" protoc
    COMMENT "Generating protobuf lite-MessageLite sources (Release)"
)


# ---------------------------------------------------------------------------
# Native SFF core — port of sff/core Python services.
# OBJECT library only: these objects are linked into 0xoCore.dll and never
# produce a fifth runtime DLL.
# ---------------------------------------------------------------------------
add_library(OxoSffNativeCore OBJECT
    sffcore/NativeCore.cpp
    sffcore/CorePaths.cpp
    sffcore/CoreProcess.cpp
    sffcore/CoreIntegrity.cpp
    sffcore/CoreSecretStore.cpp
    sffcore/CoreCache.cpp
    sffcore/KeyValues.cpp
    sffcore/CoreAcf.cpp
    sffcore/CoreIni.cpp
    sffcore/CoreSettings.cpp
    sffcore/CoreNamedIds.cpp
)

target_include_directories(OxoSffNativeCore PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_CURRENT_BINARY_DIR}/generated
)

target_compile_definitions(OxoSffNativeCore PRIVATE
    $<$<CONFIG:Debug>:OXOLEMONCORE_LOGGING_ENABLED>
    OXOLEMONCORE_DIAGNOSTICS_ENABLED
)

# Logger.h pulls spdlog headers in Debug. Linking the target here propagates
# its include directories to this OBJECT target without creating a runtime DLL.
target_link_libraries(OxoSffNativeCore PRIVATE
    $<$<CONFIG:Debug>:spdlog::spdlog>
)

# Main core DLL — output filename: 0xoCore.dll
add_library(OxoCore SHARED
    $<TARGET_OBJECTS:OxoSffNativeCore>

    core/entry.cpp
    core/Orchestrator.cpp

    runtime/Ticket.cpp
    runtime/Logger.cpp
    runtime/HookStatus.cpp
    runtime/DirWatch.cpp
    runtime/Diagnostics.cpp
    runtime/VehUtil.cpp
    runtime/RuntimeHttp.cpp
    runtime/ManifestFetch.cpp
    runtime/ProtectionProbe.cpp
    runtime/RemoteTools.cpp
    runtime/IpcSpecLoader.cpp
    runtime/BootDiag.cpp
    runtime/CredentialStore.cpp
    runtime/EticketFetcher.cpp
    runtime/TicketProvider.cpp
    runtime/ProcessInspect.cpp
    runtime/IntegrityScanner.cpp
    runtime/LibraryInjector.cpp

    patterns/ByteScan.cpp
    patterns/PatternSig.cpp
    patterns/PatternFetcher.cpp

    config/LuaState.cpp
    config/LuaBindings.cpp
    config/LuaQuery.cpp
    config/Settings.cpp

    hooks/client/DepotKeys.cpp
    hooks/client/IPCBus.cpp
    hooks/client/CmdUser.cpp
    hooks/client/CmdUtils.cpp
    hooks/client/ManifestBind.cpp
    hooks/client/PacketRouter.cpp
    hooks/client/NetPacket.cpp
    hooks/client/NetPacket_AccessToken.cpp
    hooks/client/NetPacket_UserStats.cpp
    hooks/client/NetPacket_ETicket.cpp
    hooks/client/NetPacket_Manifest.cpp
    hooks/client/NetPacket_FamilySharing.cpp
    hooks/client/NetPacket_OnlineFix.cpp
    hooks/client/NetPacket_SteamStub.cpp
    hooks/client/PackagePatch.cpp
    hooks/client/RichPresence.cpp
    hooks/client/LicenseHooks.cpp
    hooks/client/StringFind.cpp
    hooks/client/PipeWatch.cpp
    hooks/client/AuthWindow.cpp
    hooks/client/AsyncTicketMap.cpp
    hooks/client/DecryptionKeyHook.cpp
    hooks/client/PendingCallMap.cpp
    hooks/client/OnlineFixInject.cpp
    hooks/client/ProcessExtension.cpp
    hooks/client/SteamStubAuto.cpp
    hooks/client/SteamStubTicket.cpp
    hooks/client/IpcMethodLoader.cpp
    hooks/client/IpcDispatch.cpp
    hooks/client/IpcHooks.cpp
    hooks/client/IpcHandlers_ISteamUser.cpp
    hooks/client/IpcHandlers_ISteamUtils.cpp
    hooks/client/DenuvoAuthenticator.cpp

    hooks/capture/RuntimeCapture.cpp

    hooks/ui/SteamUI.cpp
    hooks/ui/StatusWriter.cpp

    $<$<CONFIG:Debug>:${PROTO_GEN_DIR}/steam_messages.pb.cc>
    $<$<CONFIG:Release>:${PROTO_GEN_LITE_DIR}/steam_messages.pb.cc>
)

# Exact public filename for the standalone core package.
set_target_properties(OxoCore PROPERTIES PREFIX "" OUTPUT_NAME "0xoCore")

target_include_directories(OxoCore PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_CURRENT_BINARY_DIR}/generated
    $<$<CONFIG:Debug>:${PROTO_GEN_DIR}>
    $<$<CONFIG:Release>:${PROTO_GEN_LITE_DIR}>
)

target_link_libraries(OxoCore PRIVATE
    lua_static
    detours
    $<$<CONFIG:Debug>:libprotobuf>
    $<$<CONFIG:Release>:libprotobuf-lite>
    tomlplusplus::tomlplusplus
    $<$<CONFIG:Debug>:spdlog::spdlog>
    winhttp
    bcrypt
    crypt32
    shell32
)

target_compile_definitions(OxoCore PRIVATE
    $<$<CONFIG:Debug>:OXOLEMONCORE_LOGGING_ENABLED>
    OXOLEMONCORE_DIAGNOSTICS_ENABLED
)

# ---------------------------------------------------------------------------
# OxoPayload — companion payload DLL
# Output filename: 0xoPayload.dll
# ---------------------------------------------------------------------------
add_library(OxoPayload SHARED
    payload/_0xoLemonCorePayload/_0xoLemonCorePayloadEntry.cpp
    payload/_0xoLemonCorePayload/LcPayloadLogging.cpp
    payload/_0xoLemonCorePayload/EpicOnlineBridge.cpp
    payload/_0xoLemonCorePayload/PayloadPropagator.cpp
)
set_target_properties(OxoPayload PROPERTIES PREFIX "" OUTPUT_NAME "0xoPayload")
target_include_directories(OxoPayload PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_CURRENT_SOURCE_DIR}/payload/_0xoLemonCorePayload
)
target_link_libraries(OxoPayload PRIVATE detours psapi)
target_compile_definitions(OxoPayload PRIVATE
    $<$<CONFIG:Debug>:OXOLEMONCORE_PAYLOAD_LOGGING_ENABLED>
)

# ---------------------------------------------------------------------------
# dwmapi proxy — shimmed dwmapi.dll
# ---------------------------------------------------------------------------
add_library(dwmapi SHARED
    proxy/LcDwmProxy.cpp
)
set_target_properties(dwmapi PROPERTIES PREFIX "" OUTPUT_NAME "dwmapi")

# ---------------------------------------------------------------------------
# xinput1_4.dll hijack — secondary loader DLL placed alongside Steam.
# ---------------------------------------------------------------------------
add_library(xinput1_4 SHARED
    proxy/LcXInputProxy.cpp
    proxy/LcXInputProxy.def
)
set_target_properties(xinput1_4 PROPERTIES PREFIX "" OUTPUT_NAME "xinput1_4")


# ---------------------------------------------------------------------------
# Standalone package: install exactly the four runtime DLLs and nothing else.
# ---------------------------------------------------------------------------
install(TARGETS OxoCore OxoPayload dwmapi xinput1_4
    RUNTIME DESTINATION .
    COMPONENT CoreRuntime
)
