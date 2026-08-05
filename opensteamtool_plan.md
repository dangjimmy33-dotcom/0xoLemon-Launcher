
---

## **Danh sách Tính năng Chi tiết**

### **1. Core Unlock System (Hệ thống Mở khóa Cơ bản)**

#### **1.1. Game Ownership Bypass**
**Mô tả:** Cho phép người dùng "sở hữu" và chạy bất kỳ game nào trên Steam mà không cần mua

**Cách hoạt động:**
- Hook các hàm kiểm tra ownership trong `ISteamApps` interface
- Inject AppId vào Package0 (danh sách game đã sở hữu) thông qua Lua script
- Sử dụng `addappid(appid [, flags] [, depotKey])` trong Lua config
- Hỗ trợ flags để kiểm soát hành vi (ví dụ: skip checks, force ownership)

**Phạm vi áp dụng:**
- Tất cả game trên Steam platform
- Hỗ trợ multiple accounts trên cùng một máy
- Tương thích với Family Sharing khi tất cả accounts đều dùng OpenSteamTool

**Implementation files:**
- `src/Hook/Hooks_Package.cpp` - Package injection
- `native/steam_hook/hooks/client/PackageInject.cpp` - Package0 manipulation
- `native/steam_hook/config/LuaBindings.cpp` - Bind_addappid function

---

#### **1.2. DLC Unlock**
**Mô tả:** Tự động mở khóa tất cả DLC của game đã được unlock

**Cách hoạt động:**
- Hook `ISteamApps::BIsDlcInstalled()` và `ISteamApps::GetDLCCount()`
- Khi game được thêm vào Package0, tất cả DLC IDs liên quan được tự động inject
- Không cần cấu hình thủ công cho từng DLC

**Phạm vi áp dụng:**
- Automatic cho mọi game đã unlock
- Bao gồm cả DLC mới release sau khi config

**Implementation files:**
- `src/Hook/Hooks_Package.cpp`
- `native/steam_emu/dll/steam_apps.cpp`

---

### **2. Depot & Manifest Management (Quản lý Phiên bản Game)**

#### **2.1. Depot Decryption Key Injection**
**Mô tả:** Tự động inject decryption keys cho depot để tải game được bảo vệ

**Cách hoạt động:**
- Lua config: `addappid(appid, 0, "decryption_key_hex")`
- Key được lưu trong memory và inject khi Steam request depot data
- Hỗ trợ multiple depots cho một game

**Phạm vi áp dụng:**
- Game có depot được mã hóa
- Game yêu cầu authentication đặc biệt

**Implementation files:**
- `native/steam_hook/hooks/client/DecryptionKeyHook.cpp`
- `src/Hook/Hooks_DecryptionKey.cpp`

---

#### **2.2. Manifest Binding (Version Pinning / Downgrade)**
**Mô tả:** **ĐÂY LÀ TÍNH NĂNG DOWNGRADE VERSION** - Cố định phiên bản cụ thể của game để ngăn auto-update

**Cách hoạt động:**
- Sử dụng `setManifestid(depotId, manifestGid [, size])` trong Lua
- Manifest GID là identifier duy nhất cho mỗi phiên bản depot
- Khi Steam request update, OpenSteamTool trả về manifest ID cũ thay vì phiên bản mới nhất
- Hỗ trợ bind multiple depots (ví dụ: game files, language packs, DLC)

**Manifest Code Fetching:**
- **Automatic APIs:** `opensteamtool`, `steamrun`, `wudrm` (configurable)
- **Custom Lua endpoints:** 
  - `fetch_manifest_code(gid)` - Basic function
  - `fetch_manifest_code_ex(app_id, depot_id, gid)` - Extended với app context
- **HTTP helpers:** `http_get(url [, headers])`, `http_post(url, body [, headers])`

**Use cases:**
- **Downgrade game:** Quay về phiên bản cũ nếu update mới có bug
- **Version lock:** Giữ phiên bản tương thích với mods
- **Bypass forced updates:** Chơi phiên bản offline khi server yêu cầu update

**Phạm vi áp dụng:**
- Tất cả game có depot ID public
- Yêu cầu có manifest GID hợp lệ (có thể get từ SteamDB hoặc manifest APIs)

**Implementation files:**
- `native/steam_hook/hooks/client/ManifestBind.cpp` - Core manifest override
- `native/steam_hook/hooks/client/ManifestFetch.cpp` - API integration
- `native/steam_hook/hooks/client/KeyValues.h` - KV patching for manifest data
- `src/Hook/Hooks_Manifest.cpp`

**Configuration example:**
```lua
-- Pin Cyberpunk 2077 depot to specific version
setManifestid(1091501, "8234567890123456789")  -- Game files
setManifestid(1091502, "8234567890123456790", 5000000)  -- Language pack với size
```

---

#### **2.3. Access Token Injection**
**Mô tả:** Thêm access token để download game/depot được bảo vệ bởi beta branches hoặc private depots

**Cách hoạt động:**
- `addtoken(appid, "token_string")`
- Token được inject vào PICS (Product Info and Change System) request
- Cho phép access beta branches không public

**Phạm vi áp dụng:**
- Beta branches cần password
- Private depots (developer/press builds)

**Implementation files:**
- `native/steam_hook/hooks/client/PICS.cpp`
- `src/Hook/Hooks_PICS.cpp`

---

### **3. DRM Protection Bypass**

#### **3.1. SteamStub Bypass**
**Mô tả:** Bypass SteamStub (Steam's built-in DRM) mà KHÔNG cần inject vào game process

**Cách hoạt động:**
- Tận dụng **off-by-four vulnerability** trong SteamDRMP ticket parsing
- Reuse local ConfigStore ticket từ Steam
- Forge AppId trong ticket để game nghĩ rằng đã được Steam verify
- Không cần AppTicket riêng cho SteamStub-only games

**Phạm vi áp dụng:**
- Game chỉ dùng SteamStub protection
- Không áp dụng cho Denuvo games

**Implementation files:**
- `native/steam_hook/hooks/client/SteamStubAuto.cpp`
- `native/steam_hook/hooks/client/SteamStubTicket.cpp`
- `src/Hook/Hooks_SteamStub.cpp`

---

#### **3.2. Denuvo Support**
**Mô tả:** Hỗ trợ chạy game có Denuvo protection bằng explicit tickets

**Cách hoạt động:**
- Yêu cầu `AppTicket` và `ETicket` hợp lệ từ account thật sở hữu game
- Lưu tickets vào Windows Credential Store: `HKCU\Software\Valve\Steam\Apps\<AppId>`
- `setAppTicket(appid, "hex_data")` và `setETicket(appid, "hex_data")`
- **extract_tickets.exe** tool để extract tickets từ máy có game

**Ticket Validity:**
- Denuvo có **30-minute validity window**
- Sau 30 phút, cần refresh ticket để tránh error `88500005`
- Tickets chỉ valid nếu extract từ account thật sở hữu game

**Ticket Priority:**
1. Explicit tickets (setAppTicket)
2. Cached credentials từ registry
3. Forged ConfigStore ticket (fallback cho SteamStub)

**SteamID Priority:**
1. Cached SteamID từ credential store
2. Parsed từ explicit AppTicket
3. Default fallback

**Phạm vi áp dụng:**
- Game có Denuvo protection
- Yêu cầu access đến máy có game genuinely owned (để extract tickets)

**Implementation files:**
- `native/steam_hook/hooks/capture/RuntimeCapture.cpp`
- `native/steam_hook/runtime/Ticket.cpp`
- `tools/extract_tickets/` - Ticket extraction utility

**Tool: extract_tickets.exe**
```powershell
# Extract tickets cho game
extract_tickets.exe 1361510

# Output:
# - <appid>/appticket.bin
# - <appid>/eticket.bin  
# - <appid>/tickets.txt (hex strings cho Lua)
```

---

### **4. Hot Reload System**

**Mô tả:** Tự động reload Lua config khi file thay đổi, không cần restart Steam

**Cách hoạt động:**
- FileWatcher monitor `<Steam>/config/lua/` và các directories được config
- Detect file add/modify/delete events
- Reload Package0, manifest bindings, tickets, keys
- Support multiple config directories với priority ordering

**Phạm vi áp dụng:**
- Tất cả Lua config files (*.lua)
- Config TOML hot-reload (`opensteamtool.toml`)

**Configuration:**
```toml
[lua]
paths = ["D:/my-steam-config/lua", "E:/shared-configs"]  # Additional directories
```

**Implementation files:**
- `native/steam_hook/config/FileWatcher.cpp`
- `native/steam_hook/config/LuaLoader.cpp`

---

### **5. Family Sharing & Remote Play Bypass**

**Mô tả:** Bypass giới hạn Family Sharing - cho phép nhiều người chơi cùng game đồng thời

**Cách hoạt động:**
- Hook ownership checks để report game như "directly owned" thay vì "family shared"
- Tất cả accounts trong Family Group phải dùng OpenSteamTool
- Bypass check "game đang được chơi bởi owner"

**Phạm vi áp dụng:**
- Game đã unlock bằng `addappid()`
- Tất cả accounts phải install OpenSteamTool

**Limitations:**
- Không hoạt động nếu chỉ một người dùng OpenSteamTool
- Steam có thể detect nếu abuse quá mức

**Implementation files:**
- `native/steam_hook/hooks/client/Ownership.cpp`
- `src/Hook/Hooks_Ownership.cpp`

---

### **6. Stats & Achievements System**

**Mô tả:** Enable achievements và stats cho game chưa sở hữu bằng cách spoof SteamID

**Cách hoạt động:**
- `setStat(appid, "target_steamid")` - Sử dụng achievement data từ SteamID khác
- Nếu không config, query `https://stats.opensteamtool.com/{appid}` (khi `enable_api = true`)
- Fallback: hardcoded SteamID `76561198028121353`

**Priority:**
1. Manual `setStat()` config
2. Stats API (`enable_api = true`)
3. Default preset SteamID

**Features:**
- Unlock achievements từ profile khác
- Sync stats (playtime, progress, etc.)
- Support legacy achievement fetch (EMSG 818 → 819)

**Configuration:**
```toml
[stats]
enable_api = true  # Query stats API for recommended SteamID
```

**Phạm vi áp dụng:**
- Game có achievements public
- Yêu cầu target SteamID có achievements unlocked

**Implementation files:**
- `CloudRedirect-new/src/common/stats_store.cpp`
- `CloudRedirect-new/src/platform/linux/achievement_inject.cpp`
- `native/steam_hook/hooks/client/Achievement.cpp`

---

### **7. Online Fix (Multiplayer Fix)**

**Mô tả:** Enable multiplayer cho game unlock bằng cách spoof AppId thành 480 (Spacewar)

**Cách hoạt động:**
- Thêm `-onlinefix` vào Steam launch parameters
- Game được launch với AppId 480 cho lobby matchmaking
- Steam nghĩ bạn đang chơi Spacewar (free game) nên allow multiplayer

**Limitations:**
- **Chỉ một game có thể dùng onlinefix đồng thời**
- Chỉ work với lobby-based matchmaking (không phải dedicated servers)
- Friends list sẽ hiển thị "Playing Spacewar" thay vì tên game thật

**Revert:**
- Remove `-onlinefix` từ launch parameters
- Multiplayer return về normal (direct connection)

**Phạm vi áp dụng:**
- Game dùng Steam lobby matchmaking
- Game KHÔNG yêu cầu ownership check trong game code

**Implementation files:**
- `native/steam_hook/hooks/client/OnlineFixInject.cpp`
- `native/steam_hook/hooks/capture/RuntimeCapture.cpp`

---

### **8. Game Process Injection**

**Mô tả:** Optional - Inject custom DLL vào game process khi game launch

**Cách hoạt động:**
- Configure trong `[inject]` section của `opensteamtool.toml`
- OpenSteamTool inject DLL based on process architecture (x86/x64)
- DLL có thể hook game functions, modify memory, etc.

**Use cases:**
- Cheat/trainer injection
- Custom patches (graphics mods, fps unlockers)
- Debugging/monitoring tools

**Configuration:**
```toml
[inject]
enabled = true
library_x64 = "MyMod.x64.dll"  # Absolute or relative to Steam root
library_x86 = "MyMod.x86.dll"
```

**Phạm vi áp dụng:**
- Mọi game process launched qua Steam
- DLL phải match architecture của game

**Implementation files:**
- `native/steam_hook/hooks/capture/RuntimeCapture.cpp`
- `native/steam_hook/platform/Process.cpp`

---

### **9. Cloud Save Redirection (CloudRedirect Integration)**

**Mô tả:** **TÍNH NĂNG ĐANG PHÁT TRIỂN** - Redirect Steam Cloud saves sang Google Drive / OneDrive / Local folder

**Cách hoạt động:**
- Load `cloud_redirect.dll` inside Steam process
- Intercept Steam Cloud RPC calls
- Route save files qua CloudRedirect engine
- Sync qua OAuth với Google Drive/OneDrive

**Features:**
- Multi-provider support (Google Drive, OneDrive, local folders)
- Quota management per-app
- Conflict resolution (local vs cloud)
- Stats sync (achievements, playtime)

**Configuration:**
```toml
[cloud]
enabled = false  # Currently experimental
library = "cloud_redirect.dll"
```

**Status:** 
- Core implementation complete trong `CloudRedirect-new/` và `native/cloud_redirect/`
- Integration với OpenSteamTool partial
- Yêu cầu companion app để OAuth sign-in

**Phạm vi áp dụng:**
- Game đã unlock với `addappid()`
- Yêu cầu CloudRedirect companion app

**Implementation files:**
- `CloudRedirect-new/src/` - Full CloudRedirect module
- `native/cloud_redirect/src/` - Native implementation
- Integration code trong OpenSteamTool hooks

---

### **10. Pattern-based Hook System (Steam Version Compatibility)**

**Mô tả:** Tự động download byte patterns cho mỗi Steam version để maintain compatibility

**Cách hoạt động:**
- Mỗi lần launch, compute SHA-256 của `steamclient64.dll` và `steamui.dll`
- Query pattern file từ upstream: `https://github.com/OpenSteam001/steam-monitor` (pattern branch)
- Fallback chain: GitHub Raw → jsDelivr CDN → Local cache
- Pattern files contain byte signatures để locate hook points

**Lookup Order:**
1. **GitHub Raw** - Canonical source
2. **jsDelivr CDN** - Auto fallback nếu GitHub blocked (China-friendly)
3. **Local Cache** - `<Steam>/opensteamtool/pattern/<subdir>/<sha256>.toml`

**Custom Mirror:**
```toml
[remote]
url_template = "https://your.mirror/{channel}/{component}/{sha256}.toml"
```

**Benefits:**
- Zero-day support cho Steam updates (nếu upstream bot đã publish pattern)
- Không cần rebuild DLL cho mỗi Steam version
- Automatic fallback khi pattern missing

**Phạm vi áp dụng:**
- Essential cho hook stability
- Yêu cầu internet connection (first launch after Steam update)

**Implementation files:**
- `native/steam_hook/patterns/PatternFetcher.cpp`
- `native/steam_hook/patterns/PatternLoader.cpp`

---

### **11. Rich Presence Manipulation**

**Mô tả:** Modify Rich Presence (status hiển thị trên Friends list)

**Cách hoạt động:**
- Hook `CMsgClientRichPresenceUpload` packets
- Spoof AppId nếu OnlineFix active
- Customize status text, game name display

**Use cases:**
- Hide real game khi dùng unlock
- Show "Playing non-Steam game" thay vì game name
- Custom status messages

**Implementation files:**
- `native/steam_hook/hooks/client/RichPresence.cpp`

---

### **12. Debugging & Logging System**

**Mô tả:** Comprehensive logging cho debugging (Debug builds only)

**Log Files** (trong `<Steam>/opensteamtool/`):
- `main.log` - Init, config, Lua parsing
- `ipc.log` - IPC commands, InterfaceCall dispatch
- `netpacket.log` - Network packet send/recv
- `manifest.log` - Manifest download/binding
- `decryptionkey.log` - Depot key injection
- `keyvalue.log` - KeyValues patching
- `achievement.log` - Stats/achievements
- `pics.log` - PICS token injection
- `package.log` - Package injection
- `onlinefix.log` - Online fix operations
- `pipe.log` - Pipe, Denuvo auth, injection
- `platform.log` - Platform helpers

**Configuration:**
```toml
[log]
level = "debug"  # trace, debug, info, warn, error
```

**Implementation files:**
- `native/steam_hook/util/Logger.cpp`
- Per-module logging macros

---

### **13. Lua Scripting Engine**

**Mô tả:** Lua 5.4 integration cho user configuration và customization

**Exposed Functions** (case-insensitive):
- `addappid(appid [, flags] [, depotKey])` - Unlock game
- `addtoken(appid, token)` - Add access token
- `setManifestid(depotId, gid [, size])` - Pin manifest version
- `setAppTicket(appid, hex)` - Set AppTicket
- `setETicket(appid, hex)` - Set ETicket
- `setStat(appid, steamid)` - Set achievement SteamID
- `http_get(url [, headers])` - HTTP GET request
- `http_post(url, body [, headers])` - HTTP POST request
- `fetch_manifest_code(gid)` - Basic manifest fetch
- `fetch_manifest_code_ex(app_id, depot_id, gid)` - Extended manifest fetch

**Features:**
- Case-insensitive function names
- Global `_G` metatable với lowercase fallback
- Hot-reload support
- Multi-directory config paths

**Implementation files:**
- `native/steam_hook/config/LuaState.cpp` - Lua VM setup
- `native/steam_hook/config/LuaBindings.cpp` - C++ ↔ Lua bindings
- `native/steam_hook/config/LuaLoader.cpp` - Config loading

---

### **14. IPC (Inter-Process Communication)**

**Mô tả:** Communication giữa Steam client và game processes

**Features:**
- Named pipe communication
- Protobuf message serialization
- RPC handlers cho cloud sync, stats, etc.

**Implementation files:**
- `native/steam_hook/hooks/client/IPCBus.cpp`
- `src/proto/` - Protobuf definitions

---

### **15. Build & Deployment System**

**Build Requirements:**
- Windows 10/11
- CMake 3.20+
- Visual Studio 2022 (MSVC x64)

**Build Script:**
```batch
build.bat  # Builds both Debug and Release
```

**Output Files:**
- `OpenSteamTool.dll` - Main hook library
- `dwmapi.dll` - Loader (Steam loads this)
- `xinput1_4.dll` - Alternative loader

**Deployment:**
1. Copy DLLs to Steam root directory
2. Create `<Steam>/config/lua/` folder
3. Place Lua configs
4. (Optional) Create `opensteamtool.toml` config

**Implementation files:**
- `build.bat` - Build script
- `src/CMakeLists.txt` - CMake configuration
- `src/dwmapi/dllmain.cpp` - Loader entry point

---

## **Kiến trúc Kỹ thuật**

### **Hook Architecture**

**Multi-layer hooking:**
1. **Loader Layer** (`dwmapi.dll`/`xinput1_4.dll`) 
   - Loaded by Steam automatically
   - Inject `OpenSteamTool.dll`

2. **Core Hook Layer** (`OpenSteamTool.dll`)
   - Hook `steamclient64.dll` functions
   - Pattern-based hook point detection
   - Detours library cho function hooking

3. **Module Hooks:**
   - **Package Hooks** - Ownership injection
   - **Manifest Hooks** - Version control
   - **IPC Hooks** - Inter-process comm
   - **Network Hooks** - Packet manipulation
   - **KeyValue Hooks** - Config patching

### **Memory Safety**

- Thread-safe atomic operations
- VEH (Vectored Exception Handling) cho crash protection
- Memory protection với VirtualProtect
- Smart pointer usage (std::unique_ptr, std::shared_ptr)

### **External Dependencies**

- **Detours** - Microsoft's hooking library
- **Lua 5.4** - Scripting engine
- **toml++** - TOML config parsing
- **protobuf** - IPC serialization
- **libcurl** - HTTP requests
- **spdlog** - Logging (alternative)

---

## **Kế hoạch Phát triển**

### **Phase 1: Core Feature Completion** ✅
- [x] Package injection (game unlock)
- [x] Manifest binding (version control)
- [x] Depot key injection
- [x] SteamStub bypass
- [x] Denuvo ticket support
- [x] Hot reload system
- [x] Pattern-based compatibility

### **Phase 2: Extended Features** ✅
- [x] Stats & achievements
- [x] Access token injection
- [x] Online fix (multiplayer)
- [x] Game process injection
- [x] Rich presence manipulation
- [x] Family sharing bypass

### **Phase 3: Cloud Integration** 🚧 (In Progress)
- [x] CloudRedirect core module
- [x] Google Drive provider
- [x] OneDrive provider
- [x] Local folder provider
- [ ] Full OpenSteamTool integration
- [ ] OAuth companion app
- [ ] Multi-account sync
- [ ] Conflict resolution UI

### **Phase 4: Enhancement & Polish** 📋 (Planned)
- [ ] GUI configuration tool
- [ ] One-click game unlock (database integration)
- [ ] Automatic manifest GID lookup
- [ ] Improved error messages
- [ ] User documentation expansion
- [ ] Video tutorials

### **Phase 5: Advanced Features** 💡 (Future)
- [ ] Linux support (Proton compatibility)
- [ ] Remote streaming optimization
- [ ] Mod manager integration
- [ ] Save game backup/restore
- [ ] Network traffic optimization
- [ ] Custom achievement tracking

---

## **Testing & Quality Assurance**

### **Testing Coverage**

**Current Status:**
- Manual testing cho core features
- Limited automated testing
- Community beta testing

**Recommended Testing:**
1. **Unit Tests** - Hook functions, Lua bindings
2. **Integration Tests** - Steam client interaction
3. **Regression Tests** - Pattern compatibility
4. **Security Tests** - Anti-cheat detection
5. **Performance Tests** - Memory/CPU overhead

### **Known Limitations**

1. **Windows Only** - No Linux/macOS support
2. **Online Fix** - Single game limitation
3. **Denuvo** - Requires genuine tickets (30-min validity)
4. **CloudRedirect** - Experimental stage
5. **Anti-cheat** - May conflict với VAC/EAC/BattlEye
6. **Steam Updates** - Dependency trên upstream pattern repo

---

## **Security & Legal Considerations**

### **Security Risks**

- DLL injection có thể trigger anti-virus
- Ticket sharing có thể violate Steam ToS
- Cloud credential storage risks
- Memory manipulation detection

### **Legal Disclaimer**

- **Educational và research purposes only**
- Users chịu trách nhiệm compliance với:
  - Local laws
  - Steam Terms of Service
  - Game EULAs
  - Copyright laws

### **Best Practices**

- Không share tickets publicly
- Không abuse multiplayer systems
- Keep tool updated
- Use trên secondary accounts
- Backup legitimate game saves

---

## **Tiêu chí Hoàn thành**

### **Feature Completeness**
- ✅ Core unlock system working
- ✅ Version control (downgrade) functional
- ✅ DRM bypass implementations
- 🚧 Cloud sync integration (partial)
- ✅ Stats/achievements system
- ✅ Hot reload working
- ✅ Multi-version Steam compatibility

### **Code Quality**
- ✅ C++20 standards compliance
- ✅ Memory-safe practices
- ⚠️ Limited test coverage
- ✅ Comprehensive logging
- ⚠️ Documentation needs expansion

### **User Experience**
- ✅ Simple Lua configuration
- ✅ Hot-reload convenience
- ⚠️ Limited GUI (command-line heavy)
- ⚠️ Error messages cần cải thiện
- ⚠️ Setup documentation cần chi tiết hơn

---

## **Kết luận**

OpenSteamTool là một dự án ambitious với **15+ major features** đã được implement. **Tính năng downgrade version** được implement hoàn chỉnh thông qua **Manifest Binding system** (`setManifestid` function), cho phép pin bất kỳ phiên bản depot nào và ngăn Steam auto-update.

**Điểm mạnh:**
- Kiến trúc modular, dễ maintain
- Pattern-based compatibility cho multiple Steam versions
- Comprehensive feature set
- Active development với upstream pattern support

**Điểm cần cải thiện:**
- Cloud sync integration chưa hoàn chỉnh
- GUI tool cho non-technical users
- Automated testing infrastructure
- Anti-cheat compatibility research
- Documentation và tutorials

**Recommended Next Steps:**
1. Complete CloudRedirect integration
2. Develop GUI configuration tool
3. Expand automated testing
4. Create comprehensive user documentation
5. Build community knowledge base

Dự án đã đạt mức độ production-ready cho core features, và đang trong giai đoạn enhancement cho advanced features như cloud sync.