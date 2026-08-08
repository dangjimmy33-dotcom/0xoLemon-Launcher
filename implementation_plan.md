# Automate Denuvo Activation Workflow in 0xoLauncher

## Goal
Tự động hóa hoàn toàn quy trình kích hoạt Denuvo (chạy game lấy ticket -> gửi server -> điền token vào file cfg -> chạy lại game) trực tiếp trong 0xoLauncher. Khách thuê game sẽ chỉ cần nhấn nút "Kích hoạt", launcher sẽ làm 100% mọi thứ.

## Proposed Changes

### `E:\007Launcher\src-tauri\src\denuvo.rs`
- Cập nhật hàm `scan_for_denuvo_ticket`: Đảm bảo chỉ đọc và trả về đúng chuỗi Ticket (dòng cuối cùng, không chứa khoảng trắng), bỏ qua các text thừa (như "EA SPORTS FC 26...").
- Cập nhật hàm `apply_denuvo_token_to_cfg`: Sử dụng `regex` để tìm và thay thế đúng vị trí `"DenuvoToken" "..."` bằng Token mới nhận được, thay vì tìm chuỗi cứng "PASTE_A_VALID...".
- **[NEW]** Thêm hàm `run_fc26_exe(game_dir: String)`: Sử dụng `std::process::Command` để gọi chạy `FC26.exe`. Lần đầu là để game tạo ra Ticket, lần sau là để vào thẳng game sau khi đã chèn Token.

### `E:\007Launcher\src\components\DenuvoActivation.tsx`
- Tích hợp luồng chạy hoàn chỉnh:
  1. Xóa file `Denuvo_ticket_*.txt` cũ (nếu có) để đảm bảo lấy ticket mới nhất.
  2. Gọi `run_fc26_exe` để game tự động sinh ra Ticket (sẽ trễ 3-5s).
  3. Quét lấy Ticket thông qua `scan_for_denuvo_ticket`.
  4. Gửi Ticket lên `0xo_token_server` và nhận Token.
  5. Ghi Token đè vào `anadius.cfg`.
  6. Gọi lại `run_fc26_exe` để khách vào thẳng game.

## User Review Required
Bạn hãy xem qua luồng 6 bước trên xem có đúng ý bạn chưa nhé? Nếu chuẩn rồi, tôi sẽ code ngay!
