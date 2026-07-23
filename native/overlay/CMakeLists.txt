cmake_minimum_required(VERSION 3.20)
project(007Overlay LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# ---------- MinHook (vendored) ----------
add_library(minhook STATIC
    vendor/minhook/src/buffer.c
    vendor/minhook/src/hook.c
    vendor/minhook/src/trampoline.c
    vendor/minhook/src/hde/hde32.c
    vendor/minhook/src/hde/hde64.c
)
target_include_directories(minhook PUBLIC vendor/minhook/include)

# ---------- Overlay DLL ----------
add_library(overlay SHARED
    src/dllmain.cpp
    src/ipc/shared_memory.cpp
    src/ipc/named_pipe.cpp
    src/hooks/dxgi_hook.cpp
    src/hooks/input_hook.cpp
    src/hooks/steam_hook.cpp
    src/renderer/dx11_renderer.cpp
)

target_link_libraries(overlay PRIVATE minhook d3d11 dxgi d3dcompiler)

# Dummy game for testing
add_executable(test_game src/test_game.cpp)
target_link_libraries(test_game PRIVATE d3d11)

# Output name depends on architecture
if(CMAKE_SIZEOF_VOID_P EQUAL 8)
    set_target_properties(overlay PROPERTIES OUTPUT_NAME "overlay64")
else()
    set_target_properties(overlay PROPERTIES OUTPUT_NAME "overlay32")
endif()

target_include_directories(overlay PRIVATE
    src
    vendor/minhook/include
)

target_link_libraries(overlay PRIVATE
    minhook
    d3d11
    dxgi
    d3dcompiler
)

# Suppress warnings for MinHook C code
target_compile_options(minhook PRIVATE /W0)
target_compile_options(overlay PRIVATE /W4)
