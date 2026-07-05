# Proxy DLL: dwmapi.dll
# This is a thin shim that forwards all DWM API calls to the real dwmapi.dll,
# then loads OxoSteamCore.dll to inject hooks into Steam

add_library(dwmapi_proxy SHARED
    dwmapi_proxy.cpp
)

set_target_properties(dwmapi_proxy PROPERTIES
    OUTPUT_NAME "dwmapi"
    PREFIX ""
    SUFFIX ".dll"
)

target_link_libraries(dwmapi_proxy PRIVATE
    user32
    kernel32
)

# Optional: xinput1_4.dll proxy as backup injection point
add_library(xinput_proxy SHARED
    xinput_proxy.cpp
)

set_target_properties(xinput_proxy PROPERTIES
    OUTPUT_NAME "xinput1_4"
    PREFIX ""
    SUFFIX ".dll"
)

target_link_libraries(xinput_proxy PRIVATE
    kernel32
)
