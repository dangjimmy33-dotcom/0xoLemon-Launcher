// <#
//     EA Denuvo Token Dumper
//     Copyright (C) 2025 RMC
//     Licensed under the Community Use License (CUL-1.0)
//     See: [https://github.com/RMC-4/EA-License-Token-Dumper/blob/main/LICENSE](https://github.com/RMC-4/EA-License-Token-Dumper/blob/main/LICENSE)
// #>

#include <windows.h>
#include <fstream>
#include <sstream>
#include <vector>
#include <regex>
#include <string>
#include <iostream>
#include <filesystem>
#include <locale>
#include <codecvt>
#include <chrono>
#include <iomanip>
#include <ctime>

// Crypto++
#include <cryptopp/aes.h>
#include <cryptopp/modes.h>
#include <cryptopp/filters.h>

// TinyXML2
#include "tinyxml2.h"

namespace fs = std::filesystem;

static std::wstring dllDir;  // Directory of the DLL
static std::wstring markerPath = fs::temp_directory_path() / L"showcase_token_dumper_marker.flag";

static std::wofstream logFile;

static void OpenLog()
{
    try {
        if (logFile.is_open())
            logFile.close();

        fs::path logPath = dllDir.empty() ? fs::current_path() : dllDir;
        logPath /= L"dumper_log.txt";

        if (!fs::exists(markerPath)) {
            // Overwrite log file if marker not present
            logFile.open(logPath, std::ios::out | std::ios::trunc);
        } else {
            // Append if marker exists
            logFile.open(logPath, std::ios::out | std::ios::app);
        }
    }
    catch (...) {
        // Logging failure ignored
    }
}

static std::wstring GetCurrentDateTime()
{
    using namespace std::chrono;
    auto now = system_clock::now();
    std::time_t now_c = system_clock::to_time_t(now);
    std::tm local_tm;
    localtime_s(&local_tm, &now_c);

    wchar_t buffer[100];
    wcsftime(buffer, sizeof(buffer)/sizeof(wchar_t), L"%Y-%m-%d %H:%M:%S", &local_tm);
    return std::wstring(buffer);
}

static void Log(const std::wstring &message)
{
    if (logFile.is_open()) {
        logFile << L"[" << GetCurrentDateTime() << L"] " << message << std::endl;
        logFile.flush();
    }
}

static void ShowMessage(const std::wstring &text, const std::wstring &title, UINT icon)
{
    Log(L"ShowMessage: " + title + L" - " + text);
    MessageBoxW(nullptr, text.c_str(), title.c_str(), MB_OK | icon);
}

static std::string ReadFileBytes(const std::wstring &path)
{
    Log(L"Reading file: " + path);
    std::ifstream file(path, std::ios::binary);
    if (!file) {
        Log(L"Failed to open file: " + path);
        return {};
    }
    std::ostringstream ss;
    ss << file.rdbuf();
    Log(L"Read " + std::to_wstring(ss.str().size()) + L" bytes");
    return ss.str();
}

static bool WriteFileText(const std::wstring &path, const std::string &text)
{
    Log(L"Writing file: " + path);
    std::ofstream file(path, std::ios::binary);
    if (!file) {
        Log(L"Failed to open file for writing: " + path);
        return false;
    }
    file.write(text.data(), text.size());
    Log(L"Wrote " + std::to_wstring(text.size()) + L" bytes");
    return true;
}

static std::string TryDecrypt(const std::string &input, const byte key[16], const byte iv[16])
{
    try {
        std::string decrypted;
        CryptoPP::CBC_Mode<CryptoPP::AES>::Decryption d;
        d.SetKeyWithIV(key, 16, iv);
        CryptoPP::StringSource ss(input, true,
            new CryptoPP::StreamTransformationFilter(d,
                new CryptoPP::StringSink(decrypted),
                CryptoPP::BlockPaddingSchemeDef::PKCS_PADDING
            )
        );
        Log(L"Decryption succeeded, size=" + std::to_wstring(decrypted.size()));
        return decrypted;
    }
    catch (...) {
        Log(L"Decryption failed");
        return {};
    }
}

static bool MarkerExists()
{
    bool exists = fs::exists(markerPath);
    Log(L"MarkerExists: " + std::wstring(exists ? L"true" : L"false"));
    return exists;
}

static void CreateMarker()
{
    Log(L"Creating marker file:" + markerPath);
    std::ofstream file(markerPath);
    file << "1";
    file.close();
}

static void RemoveMarker()
{
    if (fs::exists(markerPath)) {
        Log(L"Removing marker file:" + markerPath);
        fs::remove(markerPath);
    }
}

static void RelaunchApp()
{
    wchar_t exePath[MAX_PATH];
    if (!GetModuleFileNameW(NULL, exePath, MAX_PATH)) {
        Log(L"Failed to get module file name for relaunch");
        return;
    }
    Log(L"Relaunching app: " + std::wstring(exePath));

    STARTUPINFOW si = { sizeof(si) };
    PROCESS_INFORMATION pi = {};

    if (CreateProcessW(exePath, NULL, NULL, NULL, FALSE, 0, NULL, NULL, &si, &pi)) {
        CloseHandle(pi.hThread);
        CloseHandle(pi.hProcess);
        Log(L"Relaunch successful");
    }
    else {
        Log(L"Failed to create process for relaunch");
    }

    ExitProcess(0);
}

extern "C" __declspec(dllexport) void Run(bool overwrite = true)
{
    try
    {
        OpenLog();                  // Open log file freshly if no marker, else append
        Log(L"Run started");

        if (MarkerExists())
        {
            Log(L"Pass to game Launch: marker exists");
            RemoveMarker();
            if (logFile.is_open()) {
                logFile.close();
            }
            return;
        }

        std::wstring dlfPath = L"C:\\ProgramData\\Electronic Arts\\EA Services\\License\\16425677_sc.dlf";
        std::wstring cfgPath = dllDir + L"\\anadius.cfg";

        if (!fs::exists(dlfPath)) {
            ShowMessage(L"License file not found:\n" + dlfPath, L"Error", MB_ICONERROR);
            Log(L"License file not found: " + dlfPath);
            return;
        }

        if (overwrite && !fs::exists(cfgPath)) {
            ShowMessage(L"Config file not found:\n" + cfgPath, L"Error", MB_ICONERROR);
            Log(L"Config file not found: " + cfgPath);
            return;
        }

        const byte key[16] = { 65,50,114,45,208,130,239,176,220,100,87,197,118,104,202,9 };
        const byte iv[16] = { 0 };

        std::string fileBytes = ReadFileBytes(dlfPath);
        if (fileBytes.empty()) {
            ShowMessage(L"Failed to read license file.", L"Error", MB_ICONERROR);
            Log(L"Failed to read license file bytes");
            return;
        }

        std::string decrypted = TryDecrypt(fileBytes, key, iv);
        if (decrypted.empty()) {
            if (fileBytes.size() <= 0x41) {
                ShowMessage(L"File too small for fallback decryption.", L"Error", MB_ICONERROR);
                Log(L"File too small for fallback decryption");
                return;
            }
            std::string slice = fileBytes.substr(0x41);
            decrypted = TryDecrypt(slice, key, iv);
        }

        if (decrypted.empty()) {
            ShowMessage(L"Decryption failed. Invalid/corrupt license file.", L"Error", MB_ICONERROR);
            Log(L"Decryption failed on fallback");
            return;
        }

        tinyxml2::XMLDocument doc;
        if (doc.Parse(decrypted.c_str()) != tinyxml2::XML_SUCCESS) {
            ShowMessage(L"Decrypted data is not valid XML.", L"Error", MB_ICONERROR);
            Log(L"XML parsing failed");
            return;
        }

        tinyxml2::XMLElement* root = doc.RootElement();
        if (!root) {
            ShowMessage(L"No XML root found.", L"Error", MB_ICONERROR);
            Log(L"No XML root found");
            return;
        }

        tinyxml2::XMLElement* tokenNode = root->FirstChildElement("GameToken");
        if (!tokenNode) {
            ShowMessage(L"GameToken not found in license.", L"Error", MB_ICONERROR);
            Log(L"GameToken node not found");
            return;
        }

        std::string token = tokenNode->GetText() ? tokenNode->GetText() : "";
        token = std::regex_replace(token, std::regex("\\s+"), "");
        Log(L"Extracted token: " + std::wstring(token.begin(), token.end()));

        if (overwrite) {
            std::wstring backupPath = cfgPath + L".bak";
            fs::copy_file(cfgPath, backupPath, fs::copy_options::overwrite_existing);
            Log(L"Backup created: " + backupPath);

            std::ifstream in(cfgPath);
            std::ostringstream buffer;
            buffer << in.rdbuf();
            std::string configText = buffer.str();

            std::regex pattern(R"((\"DenuvoToken\"\s+\".*?\"))");
            std::string replacement = "\"DenuvoToken\"           \"" + token + "\"";

            if (std::regex_search(configText, pattern)) {
                std::string updated = std::regex_replace(configText, pattern, replacement);
                if (!WriteFileText(cfgPath, updated)) {
                    ShowMessage(L"Failed to update config.", L"Error", MB_ICONERROR);
                    Log(L"Failed to write updated config");
                    return;
                }
                Log(L"Config updated with new token");
            }
            else {
                ShowMessage(L"DenuvoToken entry not found in config. No changes made.", L"Info", MB_ICONINFORMATION);
                Log(L"DenuvoToken entry not found in config");
            }
        }
        else {
            ShowMessage(L"DenuvoToken extracted successfully!", L"Success", MB_ICONINFORMATION);
            Log(L"DenuvoToken extracted without overwrite");
        }

        CreateMarker();
        Log(L"Marker file created");

        RelaunchApp();
    }
    catch (const std::exception& ex)
    {
        MessageBoxA(nullptr, ex.what(), "DLL Error", MB_OK | MB_ICONERROR);
        Log(L"Exception caught: " + std::wstring_convert<std::codecvt_utf8<wchar_t>>().from_bytes(ex.what()));
    }
    catch (...)
    {
        MessageBoxA(nullptr, "Unknown DLL Error", "DLL Error", MB_OK | MB_ICONERROR);
        Log(L"Unknown exception caught");
    }
    if (logFile.is_open()) {
        logFile.close();
    }
}

BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpvReserved)
{
    switch (fdwReason) {
    case DLL_PROCESS_ATTACH:
        DisableThreadLibraryCalls(hinstDLL);
        {
            wchar_t path[MAX_PATH] = {};
            if (GetModuleFileNameW(hinstDLL, path, MAX_PATH)) {
                dllDir = fs::path(path).parent_path().wstring();
            }
            else {
                dllDir = fs::current_path().wstring(); // fallback
            }
            if (logFile.is_open())
                logFile.close();
            fs::path logPath = dllDir.empty() ? fs::current_path() : dllDir;
            logPath /= L"dumper_log.txt";
            if (!fs::exists(markerPath)) {
                logFile.open(logPath, std::ios::out | std::ios::trunc);
            }
            else {
                logFile.open(logPath, std::ios::out | std::ios::app);
            }
            Log(L"DLL loaded from: " + dllDir);
        }
        Run(true);
        break;
    case DLL_PROCESS_DETACH:
        RemoveMarker();
        Log(L"DLL detached, marker removed");
        if (logFile.is_open()) {
            logFile.close();
        }
        break;
    }
    return TRUE;
}
