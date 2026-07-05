cmake_minimum_required(VERSION 3.20)
project(OxoSteamHook VERSION 1.0.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Output directories
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)

# Fetch Microsoft Detours and build manually
include(FetchContent)
FetchContent_Declare(
    detours
    URL https://github.com/microsoft/Detours/archive/refs/tags/v4.0.1.zip
)
FetchContent_MakeAvailable(detours)

# Build Detours as static library
file(GLOB DETOURS_SOURCES 
    ${detours_SOURCE_DIR}/src/*.cpp
)
# Remove uimports.cpp - it has version check issues
list(REMOVE_ITEM DETOURS_SOURCES "${detours_SOURCE_DIR}/src/uimports.cpp")

add_library(detours_lib STATIC ${DETOURS_SOURCES})
target_include_directories(detours_lib PUBLIC ${detours_SOURCE_DIR}/src)
target_compile_definitions(detours_lib PRIVATE 
    _AMD64_
    DETOURS_64BIT=1
    DETOURS_X64=1
    WIN64
    _WIN64
    DETOURS_VERSION=0x4c0c1
)

# Fetch Lua 5.4
FetchContent_Declare(
    lua
    URL https://www.lua.org/ftp/lua-5.4.6.tar.gz
    URL_HASH SHA256=7d5ea1b9cb6aa0b59ca3dde1c6adcb57ef83a1ba8e5432c0ecd06bf439b3ad88
)
FetchContent_MakeAvailable(lua)

# Build Lua static library
file(GLOB LUA_SOURCES 
    ${lua_SOURCE_DIR}/src/*.c
)
list(REMOVE_ITEM LUA_SOURCES 
    ${lua_SOURCE_DIR}/src/lua.c
    ${lua_SOURCE_DIR}/src/luac.c
)
add_library(lua_static STATIC ${LUA_SOURCES})
target_include_directories(lua_static PUBLIC ${lua_SOURCE_DIR}/src)
target_compile_definitions(lua_static PRIVATE LUA_BUILD_AS_DLL)

# Subdirectories for proxy DLLs and main hook
add_subdirectory(proxy)
add_subdirectory(core)
