#include <regex>
#include <string>
#include <fstream>
#include <iostream>
#include <filesystem>

// these are defined in dll.cpp at the top like this:
// static char old_xxx[128] = ...
const static std::vector<std::string> interface_patterns = {
    R"(STEAMAPPS_INTERFACE_VERSION\d+)",
    R"(SteamApps\d+)",
    R"(STEAMAPPLIST_INTERFACE_VERSION\d+)",
    R"(STEAMAPPTICKET_INTERFACE_VERSION\d+)",
    R"(SteamClient\d+)",

    R"(STEAMCONTROLLER_INTERFACE_VERSION)",
    R"(SteamController\d+)",

    R"(SteamFriends\d+)",

    R"(SteamGameServerStats\d+)",
    R"(SteamGameCoordinator\d+)",
    R"(SteamGameServer\d+)",

    R"(STEAMHTMLSURFACE_INTERFACE_VERSION_\d+)",
    R"(STEAMHTTP_INTERFACE_VERSION\d+)",

    R"(SteamInput\d+)",
    R"(STEAMINVENTORY_INTERFACE_V\d+)",

    R"(SteamMatchMakingServers\d+)",
    R"(SteamMatchMaking\d+)",
    R"(SteamMatchGameSearch\d+)",

    R"(SteamParties\d+)",

    R"(STEAMMUSIC_INTERFACE_VERSION\d+)",
    R"(STEAMMUSICREMOTE_INTERFACE_VERSION\d+)",

    R"(SteamNetworkingMessages\d+)",
    R"(SteamNetworkingSockets\d+)",
    R"(SteamNetworkingUtils\d+)",
    R"(SteamNetworking\d+)",

    R"(STEAMPARENTALSETTINGS_INTERFACE_VERSION\d+)",
    R"(STEAMREMOTEPLAY_INTERFACE_VERSION\d+)",
    R"(STEAMREMOTESTORAGE_INTERFACE_VERSION\d+)",
    R"(STEAMSCREENSHOTS_INTERFACE_VERSION\d+)",

    R"(STEAMTIMELINE_INTERFACE_V\d+)",
    R"(STEAMUGC_INTERFACE_VERSION\d+)",

    R"(SteamUser\d+)",
    R"(STEAMUSERSTATS_INTERFACE_VERSION\d+)",

    R"(SteamUtils\d+)",

    R"(STEAMVIDEO_INTERFACE_V\d+)",
   
    R"(STEAMUNIFIEDMESSAGES_INTERFACE_VERSION\d+)",

    R"(SteamMasterServerUpdater\d+)", 
};

size_t findinterface(
    std::ofstream &out_file,
    const std::string &file_contents,
    const std::string &interface_patt)
{
    std::regex interface_regex(interface_patt);

    auto begin = std::sregex_iterator(file_contents.cbegin(), file_contents.cend(), interface_regex);
    auto end = std::sregex_iterator();

    std::vector<std::string> matches;
    for (std::sregex_iterator i = begin; i != end; ++i)
    {
        matches.push_back(i->str());
    }

    if (interface_patt == R"(SteamClient\d+)" &&
        matches.size() > 1 &&
        std::find(matches.begin(), matches.end(), "SteamClient017") != matches.end())
    {
        // In newer SDKs, legacy steam_api.dll interface exports were removed except for SteamClient(),
        // which still returns SteamClient017.
        auto rm = std::remove_if(matches.begin(), matches.end(), [](const std::string &item)
            {
                return (item != "SteamClient017");
            }
        );
        matches.erase(rm, matches.end());
    }

    for (const std::string &match : matches)
    {
        out_file << match << std::endl;
    }

    return matches.size();
}

int main(int argc, char *argv[])
{
    if (argc < 2)
    {
        std::cerr << "usage: " << argv[0] << " <path to steam_api .dll or .so>" << std::endl;
        return 1;
    }

    std::ifstream steam_api_file(std::filesystem::u8path(argv[1]), std::ios::binary);
    if (!steam_api_file)
    {
        std::cerr << "Error opening file: " << argv[1] << std::endl;
        return 1;
    }

    std::string steam_api_contents((std::istreambuf_iterator<char>(steam_api_file)), std::istreambuf_iterator<char>());

    steam_api_file.close();

    if (steam_api_contents.empty())
    {
        std::cerr << "Error loading data" << std::endl;
        return 1;
    }

    std::ofstream out_file("steam_interfaces.txt");
    if (!out_file)
    {
        std::cerr << "Error opening output file" << std::endl;
        return 1;
    }

    size_t total_matches = 0;

    for (const auto &patt : interface_patterns)
    {
        total_matches += findinterface(out_file, steam_api_contents, patt);
        std::cout << "Searching for '" + patt + "'..." << std::endl;
    }

    out_file.close();

    if (total_matches == 0)
    {
        std::cerr << "No interfaces were found" << std::endl;
        return 1;
    }

    return 0;
}
