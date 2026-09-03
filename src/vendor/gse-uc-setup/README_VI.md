# GSE / UC Setup V1.8.3

Tool Windows GUI để cấu hình **GSE** hoặc **UC Online2** theo hai engine tách biệt.

## Chế độ chính

### GSE
- Regular
- Experimental
- ColdClient (`steamclient_experimental` đầy đủ)
- Offline / LAN
- Achievement overlay, sound/icon config, FPS, frametime, playtime
- Steam Web API + ưu tiên `gse_fork_tools` chính chủ

### UC Online2
- Spacewar AppID 480 mặc định
- `ogAppId` = AppID thật của game
- Custom spoof AppID
- Auto/manual backend plugins: EOS, Photon, PlayFab, coherence
- UC Runtime SteamStub (`GetStubbedLol`) tùy chọn

## SteamStub

- **Auto**: thử Steamless trước; thất bại thì KHÔNG tự thả `winmm.dll`.
- **Steamless**: tạo output unpacked, backup EXE gốc cạnh file bằng `.bak`, sau đó mới replace.
- **RUNE SteamStub Patcher**: explicit proxy `winmm.dll`.
- **UC Runtime SteamStub**: runtime patch trong UC Online2.
- Disabled.

## Resource portable / hybrid

Runtime priority:

```text
<tool>\resources\updates\<component>
        ↓
embedded baseline trong GSEAutoSetup.exe
```

Không lưu package/archive/component nặng trong `%LOCALAPPDATA%` nữa. `config.ini` vẫn nằm cạnh EXE; Web API key được bảo vệ bằng Windows DPAPI.

Baseline trong source gồm full GSE, full ColdClient, Steamless CLI/plugins và migrate_gse. Builder V1.8.3 bắt buộc chuẩn bị `gse_fork_tools` chính chủ để nhúng; download có retry + `curl.exe` fallback. UC Online2/RUNE/Steamless được refresh theo component và có thể dùng baseline đang có nếu refresh mạng lỗi.

## Backup

```text
steam_api64.dll       ← file đang dùng
steam_api64.dll.bak   ← original

Game.exe              ← Steamless output (nếu dùng Steamless)
Game.exe.bak          ← original EXE

<Game>\.gse_auto_backup\  ← transaction Restore
```

Tool không ghi đè một `.bak` không thuộc nó.

## migrate_gse

Nút **Migrate old Goldberg settings** chỉ mở tool migrate chính thức khi bạn chủ động bấm. Nó không tự chạy trong Setup.

## Build EXE

Trên Windows chạy:

```bat
BUILD_EXE_V1_8_1.bat
```

Builder kiểm tra Python/PySide6, chạy test, compile check, chuẩn bị resources rồi gọi PyInstaller.

## Resource V1.8.3
Sau khi build, `dist\resources` được đặt ngay cạnh `GSEAutoSetup.exe`. Tool ưu tiên `resources\updates`, sau đó `resources\embedded` cạnh EXE, cuối cùng mới dùng baseline nhúng trong one-file EXE. Vì vậy Setup không tải lại `gse_fork_tools` nếu bản local hợp lệ đã có.

## Build V1.8.3

- `BUILD_EXE.bat`: build bình thường nhưng **không tải lại runtime resources**. Nó chỉ kiểm tra `resources\` hiện có.
- `BUILD_EXE_FAST.bat` / `BUILD_EXE_DIRECT.bat`: build nhanh bằng `.venv` + `resources` hiện có; không pip install và không truy cập GitHub.
- `UPDATE_RESOURCES.bat`: chỉ chạy file này khi muốn chủ động refresh GSE tools / UC Online2 / RUNE SteamStub / Steamless.

Sau build, luôn giữ `dist\resources` ngay cạnh `dist\GSEAutoSetup.exe`.

## Savegame Manager

Tab **Savegame Manager** quét mặc định `%APPDATA%\\GSE Saves`. Mỗi thư mục con dạng số được xem là Steam AppID và hiển thị tên/ảnh game khi Steam Store metadata khả dụng.

- **Backup**: nén nguyên thư mục AppID vào `data\\save_backups\\<appid>`.
- **Restore**: luôn tạo safety backup trước rồi thay nguyên thư mục save hiện tại.
- **Connect Google Drive**: đăng nhập OAuth trên trình duyệt, chỉ dùng scope `drive.file`.
- **Backup to Drive / Backup all**: upload resumable các snapshot ZIP đã được tạo cục bộ.
- **Restore from Drive**: tải ZIP về, kiểm tra manifest, sau đó dùng cùng transaction restore như backup local.

Google refresh token được mã hóa bằng Windows DPAPI và lưu tại `data\\google_oauth.bin`; tool không ghi `token.json` plaintext.
