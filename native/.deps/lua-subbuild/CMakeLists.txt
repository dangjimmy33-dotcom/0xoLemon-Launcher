# Distributed under the OSI-approved BSD 3-Clause License.  See accompanying
# file LICENSE.rst or https://cmake.org/licensing for details.

cmake_minimum_required(VERSION 4.3.1)

# Reject any attempt to use a toolchain file. We must not use one because
# we could be downloading it here. If the CMAKE_TOOLCHAIN_FILE environment
# variable is set, the cache variable will have been initialized from it.
unset(CMAKE_TOOLCHAIN_FILE CACHE)
unset(ENV{CMAKE_TOOLCHAIN_FILE})

# We name the project and the target for the ExternalProject_Add() call
# to something that will highlight to the user what we are working on if
# something goes wrong and an error message is produced.

project(lua-populate NONE)



include(ExternalProject)
ExternalProject_Add(lua-populate
                     "UPDATE_DISCONNECTED" "True" "DOWNLOAD_EXTRACT_TIMESTAMP" "TRUE" "EXTERNALPROJECT_INTERNAL_ARGUMENT_SEPARATOR" "URL" "https://www.lua.org/ftp/lua-5.5.0.tar.gz" "TLS_VERIFY" "OFF"
                    SOURCE_DIR          "E:/007Launcher/native/.deps/lua-src"
                    BINARY_DIR          "E:/007Launcher/native/.deps/lua-build"
                    CONFIGURE_COMMAND   ""
                    BUILD_COMMAND       ""
                    INSTALL_COMMAND     ""
                    TEST_COMMAND        ""
                    USES_TERMINAL_DOWNLOAD  YES
                    USES_TERMINAL_UPDATE    YES
                    USES_TERMINAL_PATCH     YES
)


