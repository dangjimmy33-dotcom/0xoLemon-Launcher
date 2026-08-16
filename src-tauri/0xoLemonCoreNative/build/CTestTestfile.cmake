# CMake generated Testfile for 
# Source directory: E:/007Launcher/src-tauri/0xoLemonCoreNative/source
# Build directory: E:/007Launcher/src-tauri/0xoLemonCoreNative/build
# 
# This file includes the relevant testing commands required for 
# testing this directory and lists subdirectories to be tested as well.
if(CTEST_CONFIGURATION_TYPE MATCHES "^([Dd][Ee][Bb][Uu][Gg])$")
  add_test([=[OxoStatsResponseTests]=] "E:/007Launcher/src-tauri/0xoLemonCoreNative/build/Debug/OxoStatsResponseTests.exe")
  set_tests_properties([=[OxoStatsResponseTests]=] PROPERTIES  _BACKTRACE_TRIPLES "E:/007Launcher/src-tauri/0xoLemonCoreNative/source/CMakeLists.txt;251;add_test;E:/007Launcher/src-tauri/0xoLemonCoreNative/source/CMakeLists.txt;0;")
elseif(CTEST_CONFIGURATION_TYPE MATCHES "^([Rr][Ee][Ll][Ee][Aa][Ss][Ee])$")
  add_test([=[OxoStatsResponseTests]=] "E:/007Launcher/src-tauri/0xoLemonCoreNative/build/Release/OxoStatsResponseTests.exe")
  set_tests_properties([=[OxoStatsResponseTests]=] PROPERTIES  _BACKTRACE_TRIPLES "E:/007Launcher/src-tauri/0xoLemonCoreNative/source/CMakeLists.txt;251;add_test;E:/007Launcher/src-tauri/0xoLemonCoreNative/source/CMakeLists.txt;0;")
else()
  add_test([=[OxoStatsResponseTests]=] NOT_AVAILABLE)
endif()
subdirs("E:/007Launcher/src-tauri/0xoLemonCoreNative/.deps/spdlog-build")
subdirs("E:/007Launcher/src-tauri/0xoLemonCoreNative/.deps/protobuf-build")
subdirs("E:/007Launcher/src-tauri/0xoLemonCoreNative/.deps/tomlplusplus-build")
