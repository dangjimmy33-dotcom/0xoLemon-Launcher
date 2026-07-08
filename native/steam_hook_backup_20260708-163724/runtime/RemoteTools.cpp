// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "RemoteTools.h"

#include <windows.h>
#include <tlhelp32.h>

#include <algorithm>
#include <cstring>
#include <optional>
#include <string_view>

namespace {
    struct ExportHit {
        uint32 rva = 0;
        std::string forwarder;
    };

    constexpr DWORD kInjectAccess =
        PROCESS_CREATE_THREAD |
        PROCESS_QUERY_LIMITED_INFORMATION |
        PROCESS_VM_OPERATION |
        PROCESS_VM_WRITE |
        PROCESS_VM_READ;

    std::wstring LowerWide(std::wstring value) {
        for (wchar_t& ch : value) {
            if (ch >= L'A' && ch <= L'Z')
                ch = static_cast<wchar_t>(ch - L'A' + L'a');
        }
        return value;
    }

    std::wstring FileNameOf(std::wstring_view path) {
        std::size_t pos = path.find_last_of(L"\\/");
        if (pos == std::wstring_view::npos)
            return std::wstring(path);
        return std::wstring(path.substr(pos + 1));
    }

    const void* PtrAt(const std::vector<uint8_t>& bytes, std::size_t offset, std::size_t size) {
        if (offset > bytes.size() || size > bytes.size() - offset)
            return nullptr;
        return bytes.data() + offset;
    }

    template <typename T>
    const T* StructAt(const std::vector<uint8_t>& bytes, std::size_t offset) {
        return static_cast<const T*>(PtrAt(bytes, offset, sizeof(T)));
    }

    std::optional<std::vector<uint8_t>> ReadFileBytes(const std::wstring& path) {
        HANDLE file = CreateFileW(path.c_str(), GENERIC_READ,
                                  FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                                  nullptr, OPEN_EXISTING,
                                  FILE_ATTRIBUTE_NORMAL | FILE_FLAG_SEQUENTIAL_SCAN,
                                  nullptr);
        if (file == INVALID_HANDLE_VALUE)
            return std::nullopt;

        LARGE_INTEGER size{};
        if (!GetFileSizeEx(file, &size) || size.QuadPart <= 0 || size.QuadPart > 96ll * 1024ll * 1024ll) {
            CloseHandle(file);
            return std::nullopt;
        }

        std::vector<uint8_t> bytes(static_cast<std::size_t>(size.QuadPart));
        std::size_t done = 0;
        while (done < bytes.size()) {
            DWORD want = static_cast<DWORD>((std::min)(bytes.size() - done,
                                                       static_cast<std::size_t>(1u << 20)));
            DWORD got = 0;
            if (!ReadFile(file, bytes.data() + done, want, &got, nullptr) || got == 0) {
                CloseHandle(file);
                return std::nullopt;
            }
            done += got;
        }

        CloseHandle(file);
        return bytes;
    }

    std::optional<std::size_t> RvaToRaw(const std::vector<uint8_t>& bytes,
                                        const IMAGE_SECTION_HEADER* sections,
                                        WORD sectionCount,
                                        uint32 rva) {
        for (WORD i = 0; i < sectionCount; ++i) {
            const auto& section = sections[i];
            uint32 span = (std::max)(section.Misc.VirtualSize, section.SizeOfRawData);
            uint32 begin = section.VirtualAddress;
            uint32 end = begin + span;
            if (end < begin || rva < begin || rva >= end)
                continue;

            uint32 delta = rva - begin;
            if (delta >= section.SizeOfRawData)
                return std::nullopt;

            std::size_t raw = static_cast<std::size_t>(section.PointerToRawData) + delta;
            if (raw >= bytes.size())
                return std::nullopt;
            return raw;
        }
        return std::nullopt;
    }

    const char* CStringAt(const std::vector<uint8_t>& bytes, std::size_t offset) {
        if (offset >= bytes.size())
            return nullptr;
        const char* text = reinterpret_cast<const char*>(bytes.data() + offset);
        const void* end = std::memchr(text, '\0', bytes.size() - offset);
        return end ? text : nullptr;
    }

    std::optional<ExportHit> FindExportInFile(const std::wstring& path,
                                              std::string_view symbol) {
        auto file = ReadFileBytes(path);
        if (!file)
            return std::nullopt;
        const auto& bytes = *file;

        const auto* dos = StructAt<IMAGE_DOS_HEADER>(bytes, 0);
        if (!dos || dos->e_magic != IMAGE_DOS_SIGNATURE || dos->e_lfanew < 0)
            return std::nullopt;

        std::size_t ntOffset = static_cast<std::size_t>(dos->e_lfanew);
        const auto* sig = StructAt<DWORD>(bytes, ntOffset);
        if (!sig || *sig != IMAGE_NT_SIGNATURE)
            return std::nullopt;

        std::size_t fileHeaderOffset = ntOffset + sizeof(DWORD);
        const auto* fh = StructAt<IMAGE_FILE_HEADER>(bytes, fileHeaderOffset);
        if (!fh || fh->NumberOfSections == 0)
            return std::nullopt;

        std::size_t optionalOffset = fileHeaderOffset + sizeof(IMAGE_FILE_HEADER);
        const auto* magic = StructAt<WORD>(bytes, optionalOffset);
        if (!magic)
            return std::nullopt;

        IMAGE_DATA_DIRECTORY exportsDir{};
        if (*magic == IMAGE_NT_OPTIONAL_HDR64_MAGIC) {
            const auto* opt = StructAt<IMAGE_OPTIONAL_HEADER64>(bytes, optionalOffset);
            if (!opt) return std::nullopt;
            exportsDir = opt->DataDirectory[IMAGE_DIRECTORY_ENTRY_EXPORT];
        } else if (*magic == IMAGE_NT_OPTIONAL_HDR32_MAGIC) {
            const auto* opt = StructAt<IMAGE_OPTIONAL_HEADER32>(bytes, optionalOffset);
            if (!opt) return std::nullopt;
            exportsDir = opt->DataDirectory[IMAGE_DIRECTORY_ENTRY_EXPORT];
        } else {
            return std::nullopt;
        }

        std::size_t sectionsOffset = optionalOffset + fh->SizeOfOptionalHeader;
        const auto* sections = StructAt<IMAGE_SECTION_HEADER>(bytes, sectionsOffset);
        if (!sections ||
            sectionsOffset + static_cast<std::size_t>(fh->NumberOfSections) * sizeof(IMAGE_SECTION_HEADER) > bytes.size())
            return std::nullopt;

        auto exportRaw = RvaToRaw(bytes, sections, fh->NumberOfSections, exportsDir.VirtualAddress);
        if (!exportRaw)
            return std::nullopt;

        const auto* exports = StructAt<IMAGE_EXPORT_DIRECTORY>(bytes, *exportRaw);
        if (!exports || exports->NumberOfNames == 0 || exports->NumberOfFunctions == 0)
            return std::nullopt;

        auto namesRaw = RvaToRaw(bytes, sections, fh->NumberOfSections, exports->AddressOfNames);
        auto ordRaw = RvaToRaw(bytes, sections, fh->NumberOfSections, exports->AddressOfNameOrdinals);
        auto funcsRaw = RvaToRaw(bytes, sections, fh->NumberOfSections, exports->AddressOfFunctions);
        if (!namesRaw || !ordRaw || !funcsRaw)
            return std::nullopt;

        for (DWORD i = 0; i < exports->NumberOfNames; ++i) {
            const auto* nameRva = StructAt<DWORD>(bytes, *namesRaw + static_cast<std::size_t>(i) * sizeof(DWORD));
            if (!nameRva)
                return std::nullopt;
            auto nameRaw = RvaToRaw(bytes, sections, fh->NumberOfSections, *nameRva);
            const char* exportedName = nameRaw ? CStringAt(bytes, *nameRaw) : nullptr;
            if (!exportedName || symbol != exportedName)
                continue;

            const auto* ord = StructAt<WORD>(bytes, *ordRaw + static_cast<std::size_t>(i) * sizeof(WORD));
            if (!ord || *ord >= exports->NumberOfFunctions)
                return std::nullopt;
            const auto* functionRva = StructAt<DWORD>(bytes, *funcsRaw + static_cast<std::size_t>(*ord) * sizeof(DWORD));
            if (!functionRva || *functionRva == 0)
                return std::nullopt;

            ExportHit hit{};
            hit.rva = *functionRva;
            uint32 exportEnd = exportsDir.VirtualAddress + exportsDir.Size;
            if (exportEnd >= exportsDir.VirtualAddress &&
                *functionRva >= exportsDir.VirtualAddress &&
                *functionRva < exportEnd) {
                auto fwdRaw = RvaToRaw(bytes, sections, fh->NumberOfSections, *functionRva);
                const char* fwd = fwdRaw ? CStringAt(bytes, *fwdRaw) : nullptr;
                if (!fwd)
                    return std::nullopt;
                hit.forwarder = fwd;
            }
            return hit;
        }
        return std::nullopt;
    }

    std::wstring ForwardModuleName(std::string_view value) {
        std::wstring out;
        out.reserve(value.size() + 4);
        for (char ch : value)
            out.push_back(static_cast<unsigned char>(ch));
        if (out.find(L'.') == std::wstring::npos)
            out += L".dll";
        return out;
    }

    const RemoteTools::ModuleInfo* ModuleByName(const std::vector<RemoteTools::ModuleInfo>& modules,
                                                std::wstring_view wantedName) {
        std::wstring wanted = LowerWide(std::wstring(wantedName));
        for (const auto& mod : modules) {
            if (LowerWide(mod.name) == wanted || LowerWide(FileNameOf(mod.path)) == wanted)
                return &mod;
        }
        return nullptr;
    }

    std::optional<uintptr_t> RemoteExportAddress(const std::vector<RemoteTools::ModuleInfo>& modules,
                                                 std::wstring moduleName,
                                                 std::string symbol,
                                                 int depth = 0) {
        if (depth > 8)
            return std::nullopt;

        const RemoteTools::ModuleInfo* module = ModuleByName(modules, moduleName);
        if (!module && LowerWide(moduleName).rfind(L"api-ms-win-", 0) == 0)
            module = ModuleByName(modules, L"kernelbase.dll");
        if (!module)
            return std::nullopt;

        auto hit = FindExportInFile(module->path, symbol);
        if (!hit)
            return std::nullopt;

        if (hit->forwarder.empty())
            return module->base + hit->rva;

        std::size_t dot = hit->forwarder.find('.');
        if (dot == std::string::npos || dot == 0 || dot + 1 >= hit->forwarder.size())
            return std::nullopt;

        std::string_view fwdModule(hit->forwarder.data(), dot);
        std::string_view fwdSymbol(hit->forwarder.data() + dot + 1,
                                   hit->forwarder.size() - dot - 1);
        if (!fwdSymbol.empty() && fwdSymbol.front() == '#')
            return std::nullopt;

        return RemoteExportAddress(modules,
                                   ForwardModuleName(fwdModule),
                                   std::string(fwdSymbol),
                                   depth + 1);
    }

    std::optional<LPTHREAD_START_ROUTINE> RemoteLoadLibraryW(uint32 pid,
                                                             std::string& error) {
        auto modules = RemoteTools::EnumerateModules(pid);
        auto remoteLoad = RemoteExportAddress(modules, L"kernel32.dll", "LoadLibraryW");
        if (!remoteLoad) {
            error = "remote LoadLibraryW export lookup failed";
            return std::nullopt;
        }
        return reinterpret_cast<LPTHREAD_START_ROUTINE>(*remoteLoad);
    }
}

namespace RemoteTools {
    const char* BitsName(ProcessBits bits) {
        switch (bits) {
            case ProcessBits::X86: return "x86";
            case ProcessBits::X64: return "x64";
            default: return "unknown";
        }
    }

    ProcessBits DetectBits(uint32 pid) {
        HANDLE process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
        if (!process)
            return ProcessBits::Unknown;

        using IsWow64Process2_t = BOOL(WINAPI*)(HANDLE, USHORT*, USHORT*);
        auto fn = reinterpret_cast<IsWow64Process2_t>(
            GetProcAddress(GetModuleHandleW(L"kernel32.dll"), "IsWow64Process2"));
        if (fn) {
            USHORT processMachine = IMAGE_FILE_MACHINE_UNKNOWN;
            USHORT nativeMachine = IMAGE_FILE_MACHINE_UNKNOWN;
            if (fn(process, &processMachine, &nativeMachine)) {
                CloseHandle(process);
                if (processMachine == IMAGE_FILE_MACHINE_I386)
                    return ProcessBits::X86;
                if (processMachine == IMAGE_FILE_MACHINE_UNKNOWN &&
                    nativeMachine == IMAGE_FILE_MACHINE_AMD64)
                    return ProcessBits::X64;
                return ProcessBits::Unknown;
            }
        }

        BOOL wow64 = FALSE;
        ProcessBits bits = ProcessBits::Unknown;
        if (IsWow64Process(process, &wow64))
            bits = wow64 ? ProcessBits::X86 : ProcessBits::X64;
        CloseHandle(process);
        return bits;
    }

    std::vector<ModuleInfo> EnumerateModules(uint32 pid) {
        std::vector<ModuleInfo> modules;
        HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);
        if (snap == INVALID_HANDLE_VALUE)
            return modules;

        MODULEENTRY32W entry{};
        entry.dwSize = sizeof(entry);
        if (Module32FirstW(snap, &entry)) {
            do {
                ModuleInfo mod{};
                mod.name = entry.szModule;
                mod.path = entry.szExePath[0] ? entry.szExePath : entry.szModule;
                mod.base = reinterpret_cast<uintptr_t>(entry.modBaseAddr);
                mod.size = entry.modBaseSize;
                modules.push_back(std::move(mod));
                entry.dwSize = sizeof(entry);
            } while (Module32NextW(snap, &entry));
        }
        CloseHandle(snap);
        return modules;
    }

    bool HasModuleFileName(const std::vector<ModuleInfo>& modules,
                           const std::filesystem::path& dllPath) {
        std::wstring wanted = LowerWide(dllPath.filename().wstring());
        for (const auto& mod : modules) {
            if (LowerWide(FileNameOf(mod.path)) == wanted)
                return true;
        }
        return false;
    }

    LoadResult LoadLibraryInto(uint32 pid, const std::filesystem::path& dllPath) {
        LoadResult result{};
        auto modules = EnumerateModules(pid);
        if (HasModuleFileName(modules, dllPath)) {
            result.ok = true;
            result.alreadyLoaded = true;
            return result;
        }

        HANDLE process = OpenProcess(kInjectAccess, FALSE, pid);
        if (!process) {
            result.error = "OpenProcess failed err=" + std::to_string(GetLastError());
            return result;
        }

        const std::wstring nativePath = dllPath.wstring();
        const SIZE_T bytes = (nativePath.size() + 1) * sizeof(wchar_t);
        void* remote = VirtualAllocEx(process, nullptr, bytes,
                                      MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        if (!remote) {
            result.error = "VirtualAllocEx failed err=" + std::to_string(GetLastError());
            CloseHandle(process);
            return result;
        }

        SIZE_T written = 0;
        if (!WriteProcessMemory(process, remote, nativePath.c_str(), bytes, &written) ||
            written != bytes) {
            result.error = "WriteProcessMemory failed err=" + std::to_string(GetLastError());
            VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            CloseHandle(process);
            return result;
        }

        auto loadLibraryW = RemoteLoadLibraryW(pid, result.error);
        if (!loadLibraryW) {
            VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            CloseHandle(process);
            return result;
        }

        HANDLE thread = CreateRemoteThread(process, nullptr, 0,
                                           *loadLibraryW, remote, 0, nullptr);
        if (!thread) {
            result.error = "CreateRemoteThread failed err=" + std::to_string(GetLastError());
            VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            CloseHandle(process);
            return result;
        }

        DWORD wait = WaitForSingleObject(thread, 10'000);
        if (wait != WAIT_OBJECT_0) {
            result.error = wait == WAIT_TIMEOUT
                ? "remote LoadLibraryW timed out"
                : "remote LoadLibraryW wait failed err=" + std::to_string(GetLastError());
            CloseHandle(thread);
            VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            CloseHandle(process);
            return result;
        }

        DWORD exitCode = 0;
        GetExitCodeThread(thread, &exitCode);
        CloseHandle(thread);
        VirtualFreeEx(process, remote, 0, MEM_RELEASE);
        CloseHandle(process);

        if (exitCode == 0) {
            result.error = "remote LoadLibraryW returned null";
            return result;
        }

        result.ok = true;
        return result;
    }
}

