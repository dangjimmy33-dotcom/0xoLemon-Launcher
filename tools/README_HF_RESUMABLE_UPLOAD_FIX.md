# 0xo Depot Tool - HF upload stability fix

Fix này đổi safe publish path sang:
1. Build depot local trước, không để depot_builder tự upload từng pack.
2. Check pack mã hóa local.
3. Upload bằng `hf_resumable_upload_depot.py` qua `HfApi.upload_large_folder`.
4. Tắt progress bar spam trong GUI bằng `HF_HUB_DISABLE_PROGRESS_BARS=1`.
5. Bật `HF_XET_HIGH_PERFORMANCE=1` mặc định cho upload lớn.
6. Server local strip ANSI escape codes để log không còn đầy `[A`.

Nếu upload bị ngắt, chạy lại cùng cấu hình. `upload_large_folder` lưu cache trong:
`<DepotRoot>\<GameId>\.hf_upload_stage\.cache\.huggingface`
để resume các task đã xong.

Lưu ý: hãy dùng cho nội dung/tài nguyên bạn có quyền phân phối.
