# OxoSteamCore.dll - Main hook library
# Uses Microsoft Detours to hook Steam ownership functions

add_library(OxoSteamCore SHARED
    entry.cpp
    lua_loader.cpp
    lua_loader.h
    pattern_scanner.cpp
    pattern_scanner.h
    hooks/package_hooks.cpp
    hooks/package_hooks.h
)

target_include_directories(OxoSteamCore PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${detours_SOURCE_DIR}/src
)

target_link_libraries(OxoSteamCore PRIVATE
    detours_lib
    lua_static
    psapi
)

# Export initialization functions
set_target_properties(OxoSteamCore PROPERTIES
    OUTPUT_NAME "OxoSteamCore"
    PREFIX ""
    SUFFIX ".dll"
)

target_compile_definitions(OxoSteamCore PRIVATE
    _CRT_SECURE_NO_WARNINGS
    UNICODE
    _UNICODE
)
