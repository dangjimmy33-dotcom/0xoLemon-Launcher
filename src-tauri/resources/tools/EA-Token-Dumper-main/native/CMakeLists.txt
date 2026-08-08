cmake_minimum_required(VERSION 3.16)
project(ShowcaseTokenDumper LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_WINDOWS_EXPORT_ALL_SYMBOLS ON)

# Your DLL target
add_library(ShowcaseTokenDumper SHARED src/TokenDumper.cpp)

# Find vcpkg dependencies
find_package(tinyxml2 CONFIG REQUIRED)
find_package(cryptopp CONFIG REQUIRED)

# Link to vcpkg libraries
target_link_libraries(ShowcaseTokenDumper PRIVATE tinyxml2::tinyxml2 cryptopp::cryptopp user32)
