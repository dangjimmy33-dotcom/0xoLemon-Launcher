# 0xoLemon Steam Region UI Fix

Bản này được vá trên đúng `tools(4).zip` mới nhất bạn gửi.

## Sửa gì

- Thêm lại dropdown **Vùng Steam metadata** trong `asset-builder-gui-local.html`.
- `Xác thực` AppID gọi `/api/steam/validate?...&country=us` thay vì thiếu country.
- `Tải tài sản` và `Gói xây dựng` truyền `steamCountry` xuống backend.
- `asset-builder-gui.html` bản copy-command cũng thêm `-SteamCountry`.
- Giữ fix PowerShell `${cc}` / `${lang}` để không lỗi parser khi có dấu `:`.

## Cách dùng

Giải nén vào thẳng `E:\007Launcher`, cho ghi đè thư mục `tools`.

Sau đó chạy lại:

```powershell
cd E:\007Launcher
.\tools\run-local-tool.bat
```

Với Stellar Blade:

- AppID: `3489700`
- Vùng Steam metadata: `US - ưu tiên chống region lock`

