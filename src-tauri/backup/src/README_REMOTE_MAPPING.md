# 0xoLemon backend mapping table fix

Bản này thêm bảng mapping tập trung ở:

```txt
src-tauri/src/remote_paths.rs
```

Từ giờ khi thêm game mới, sửa đúng 1 chỗ:

```rust
pub const GAME_PATH_MAPPINGS: &[GamePathMapping] = &[
    GamePathMapping {
        game_id: "new-game-id",
        install_dir_name: "New Game Name",
        hf_dir_name: "New-Game-Name-On-HF",
        launch_executable: "New Game Name.exe",
    },
];
```

Ý nghĩa:

- `game_id`: id nội bộ launcher, ví dụ `geometry-dash`.
- `install_dir_name`: tên folder local trong `E:\0xoLemon store\common`.
- `hf_dir_name`: tên folder thật trên Hugging Face. Phần này phải đúng chữ hoa/thường.
- `launch_executable`: exe mặc định để Play game.

Các file đã được chỉnh để dùng bảng mapping này:

```txt
src-tauri/src/lib.rs
src-tauri/src/remote_paths.rs
src-tauri/src/job/paths.rs
src-tauri/src/asset_pack.rs
src-tauri/src/asset_pack/generic_source.rs
```

Build:

```powershell
cd E:\007Launcher
npm run tauri build
```

Nếu game tải không được, kiểm tra trước tiên `hf_dir_name` có khớp folder thật trên Hugging Face không.
