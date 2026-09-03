// cloud_redirect_cli.exe - Windows CLI wrapper for cloud_redirect.dll
// This tiny executable loads the DLL and calls the CLI entry point.

#define WIN32_LEAN_AND_MEAN
#include <Windows.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>

typedef int (*CliMainFn)(int argc, char** argv);

int main(int argc, char** argv) {
    // Load the DLL from the same directory as this executable
    char dllPath[MAX_PATH];
    DWORD len = GetModuleFileNameA(nullptr, dllPath, MAX_PATH);
    if (len == 0 || len >= MAX_PATH) {
        fprintf(stderr, "Error: Cannot determine executable path\n");
        return 1;
    }
    
    // The launcher brands the embedded DLL while upstream releases retain the
    // original filename. Accept both layouts so the same CLI remains useful in
    // packaged and standalone CloudRedirect installations.
    char* lastSlash = strrchr(dllPath, '\\');
    const char* dllNames[] = { "0xoCloudRedirect.dll", "cloud_redirect.dll" };
    HMODULE hDll = nullptr;
    DWORD loadError = ERROR_MOD_NOT_FOUND;
    for (const char* dllName : dllNames) {
        if (lastSlash) {
            strcpy_s(lastSlash + 1, MAX_PATH - static_cast<size_t>(lastSlash + 1 - dllPath), dllName);
        } else {
            strcpy_s(dllPath, MAX_PATH, dllName);
        }
        hDll = LoadLibraryA(dllPath);
        if (hDll) break;
        loadError = GetLastError();
    }

    if (!hDll) {
        fprintf(stderr, "Error: Cannot load CloudRedirect engine beside %s (error %lu)\n",
                argv[0], loadError);
        return 1;
    }
    
    CliMainFn cliMain = (CliMainFn)GetProcAddress(hDll, "CloudRedirect_CliMain");
    if (!cliMain) {
        fprintf(stderr, "Error: Cannot find CloudRedirect_CliMain in DLL\n");
        FreeLibrary(hDll);
        return 1;
    }
    
    // Prepend "--cli" to arguments if not already present
    // Allows: cloud_redirect_cli auth-status gdrive
    // Instead of: cloud_redirect_cli --cli auth-status gdrive
    int exitCode;
    if (argc >= 2 && strcmp(argv[1], "--cli") == 0) {
        exitCode = cliMain(argc, argv);
    } else {
        // Build new argv with --cli inserted
        char** newArgv = (char**)malloc((argc + 2) * sizeof(char*));
        newArgv[0] = argv[0];
        newArgv[1] = (char*)"--cli";
        for (int i = 1; i < argc; i++) {
            newArgv[i + 1] = argv[i];
        }
        newArgv[argc + 1] = nullptr;
        exitCode = cliMain(argc + 1, newArgv);
        free(newArgv);
    }
    
    FreeLibrary(hDll);
    return exitCode;
}
