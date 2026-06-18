# 0xo Asset Builder Local Tool

## Cách chạy

1. Giải nén thư mục này.
2. Chạy `run-local-tool.bat`.
3. Mở `http://127.0.0.1:8765` nếu trình duyệt chưa tự mở.
4. Nhập Steam App ID, bấm **Xác thực**, rồi bấm **Tải tài sản**.

## Bản này đã sửa gì?

- GUI chạy qua backend Python local nên không còn lỗi CORS khi xác thực Steam App ID.
- Sửa endpoint SteamGridDB: dùng dạng `/api/v2/grids/steam/{appid}`, `/heroes/steam/{appid}`, `/logos/steam/{appid}`, `/icons/steam/{appid}`.
- `background-raw` và poster video bị 404 sẽ được coi là asset phụ, không làm đỏ log như lỗi chính.
- Poster video sẽ fallback sang thumbnail nếu Steam không có `movie_600x337.jpg`.
- Video xuất MP4/H.264 để dễ phát hơn trong launcher/webview.

## Yêu cầu

- Windows có Python 3.
- PowerShell hoặc PowerShell 7.
- `ffmpeg` trong PATH.
- `cargo` trong PATH nếu muốn bấm **Gói xây dựng**.

## Ghi chú

Nếu SteamGridDB vẫn báo `[sgdb skip]`, kiểm tra lại API key SteamGridDB. Tool vẫn có fallback từ Steam CDN nên vẫn tải được grid/hero/logo/icon cơ bản.

## Hotfix 0-byte WebP

Bản này đã sửa lỗi ffmpeg/SteamGridDB tạo file `.webp` 0 byte nhưng script vẫn tưởng là thành công.
Khi gặp file rỗng/hỏng, tool sẽ tự xóa và tải lại, rồi thử URL fallback tiếp theo.
Nếu đang có file 0 byte từ lần chạy cũ, chỉ cần chạy lại `Tải tài sản`; không cần xóa tay.

## Hotfix video/build

Bản này thêm bước kiểm tra MP4 trước khi chạy `asset_pack_builder`:
- Xóa file tạm kiểu `*.source.mp4`, `*.tmp.mp4`, `*.repaired.tmp.mp4` nếu còn sót.
- Quét toàn bộ `Assets Root` để tìm MP4 có cảnh báo decode như `Invalid NAL unit size`.
- Tự remux/re-encode file lỗi sang MP4 H.264/AAC sạch trước khi build.

Lưu ý: nút `Gói xây dựng` có thể build toàn bộ thư mục assets, nên cảnh báo video có thể đến từ game khác trong `E:\007Launcher\src\assets`, không nhất thiết từ game đang chọn.

## Hotfix build một game

- Nút **Gói xây dựng** bây giờ chỉ build game đang nhập/chọn trong GUI.
- Backend sẽ tạm ẩn các thư mục asset game khác sang thư mục hold nằm cạnh `assets`, chạy `asset_pack_builder`, rồi tự khôi phục lại sau khi build xong hoặc lỗi.
- Nút **Xây dựng tất cả các gói** mới chạy build toàn bộ `Assets Root` như trước.
- Kiểm tra/sửa MP4 trước build cũng chỉ chạy trong thư mục game đang build, tránh bị dính video lỗi từ game khác đã build rồi.

## Hotfix chọn thư mục khi build

- Khi bấm **Gói xây dựng**, GUI sẽ mở hộp chọn thư mục asset trong `Assets Root`.
- Danh sách tự đọc các thư mục đã fetch, hiển thị AppID, số screenshot/video/achievement/media.
- Chọn đúng game cần đóng gói rồi bấm **Gói thư mục đã chọn**.
- Backend vẫn tạm ẩn các thư mục khác trong lúc build để `asset_pack_builder` chỉ thấy đúng một game.
