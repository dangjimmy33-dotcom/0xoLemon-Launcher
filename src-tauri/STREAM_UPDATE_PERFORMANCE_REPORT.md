# 0xoLemon Launcher — Stream Update performance fix

## Kết luận điều tra

Pha `Stream update` không chậm chủ yếu vì tải mạng. Với game có hàng nghìn file nhỏ, luồng cũ tạo quá nhiều “điểm đồng bộ cưỡng bức” và quá nhiều lần ghi lại JSON trạng thái đang lớn dần:

1. Downloader được lập kế hoạch theo từng file, nên game 7.000 file có thể tạo hàng nghìn đợt download rất nhỏ.
2. `current-job.json` bị ghi + `sync_all()` theo callback tiến trình và sau từng file.
3. `state.json` bị ghi lại sau checkpoint/hoàn tất của từng file.
4. `txn.json` bị ghi lại nhiều lần cho mỗi file: intent, backup, installed. Vì mảng transaction lớn dần, tổng lượng JSON serialize có tính chất gần O(n²).
5. Mỗi file bị `sync_data`, tiếp theo `sync_all`, rồi mở lại và đọc toàn bộ để SHA-256.
6. Khi commit, toàn bộ map nguồn chunk bị quét lại theo từng file; với nhiều file/chunk, chi phí tăng rất mạnh.

## Codex đã cải tiến trước đó

Bản nguồn nhận được đã có một kiến trúc update khá chắc chắn:

- byte-range download có `.part`, checkpoint và resume;
- gom range theo pack, worker song song, retry/rate-limit và adaptive range;
- tái sử dụng chunk cục bộ, kể cả phát hiện chunk bị dịch offset bằng CDC;
- sequential short-path staging để tránh MAX_PATH;
- transaction backup/rollback và install marker làm commit point;
- phục hồi job/session sau khi launcher bị tắt;
- tách `job.rs` thành các module `direct`, `paths`, `progress`, `sequential`;
- ước lượng dung lượng tạm, pause/cancel, verify và delta/patch flow.

Phần còn thiếu là tối ưu trường hợp **rất nhiều file nhỏ**.

## Thay đổi bổ sung trong bản này

### 1. Rolling cross-file prefetch

- Prefetch tối đa 256 file và bị chặn bởi `download_queue_mb`.
- Dùng high-water mark `prefetched_through`; không khởi động lại downloader cho từng file đã nằm trong cửa sổ prefetch.
- Tính byte thiếu theo kiểu incremental và deduplicate hash, không gọi lại planner trên toàn bộ cửa sổ sau mỗi file.

### 2. Batch commit

- Giữ tối đa 256 file hoặc một `queue_budget` dữ liệu staging rồi commit theo batch.
- Ghi toàn bộ transaction intent của batch một lần trước rename đầu tiên.
- Không fsync `txn.json` sau từng trạng thái `BackedUp`/`Installed`.
- Vẫn giữ rollback: intent đã durable trước khi file đầu tiên bị đổi tên.

### 3. Giảm flush/fsync cưỡng bức

- `finish_writer` chỉ `flush()`; dữ liệu của cả batch được `sync_data()` ngay trước commit.
- Đồng bộ staging bằng tối đa 8 worker.
- Checkpoint rõ ràng vẫn `sync_data()` và persist state ngay, nên resume boundary không bị yếu đi.
- `state.json` được debounce: 2 giây hoặc 64 MiB thay đổi.
- Job journal persist mỗi 2 giây; UI vẫn emit tối đa mỗi 200 ms.

### 4. Bỏ lần đọc lại toàn file không cần thiết

- SHA-256 toàn file được cập nhật incremental trong lúc append chunk.
- Mỗi chunk vẫn được verify trước khi ghi.
- Khi resume, phần durable được đọc lại và verify để tái tạo hasher.
- Tránh mở/đọc lại toàn bộ 7.000 file ngay sau khi vừa ghi xong.

### 5. Bỏ quét map chunk theo từng file

- Tạo bảng `old_path -> new_path` cho cả batch.
- Quét `local_sources` đúng một lần mỗi batch, thay vì một hoặc hai lần mỗi file.

### 6. Giảm log/UI overhead

- Không thêm một log entry cho mọi file; chỉ log file đầu, cuối và mỗi 64 file.
- JSON journal/state/transaction chuyển từ pretty JSON sang compact JSON.

## Mô hình 7.000 file

Đây là mô hình số lần thao tác theo source, không phải benchmark thời gian thực tế:

- transaction snapshots: khoảng `21.000` -> khoảng `28` batch intent snapshots;
- finish-state writes: khoảng `14.000` -> bị giới hạn theo 2 giây/64 MiB, cộng checkpoint thực sự;
- downloader windows cho file nhỏ: có thể gần `7.000` -> tối đa khoảng `28` cửa sổ theo giới hạn 256 file;
- source-map remap: từ quét map theo từng file -> một lần mỗi batch.

Tốc độ thực tế còn phụ thuộc SSD/HDD, antivirus, kích thước chunk, pack layout, proxy và số file thật sự thay đổi.

## An toàn và recovery

Các invariant được giữ lại:

- chunk phải hợp lệ trước khi append;
- full-file SHA-256 vẫn phải khớp manifest;
- checkpoint resume vẫn durable;
- transaction intent của toàn batch durable trước rename;
- crash trước install marker sẽ rollback;
- crash sau install marker sẽ coi target là committed và chỉ cleanup backup.

Đã bổ sung regression test `batch_intent_is_durable_before_the_first_file_rename` và mở rộng test resume để xác nhận incremental SHA-256 sau khi reopen.

## Kiểm tra đã chạy trong môi trường này

- kiểm tra delimiter/cấu trúc Rust cho `job.rs` và `job/sequential.rs`;
- `git diff --check`: không có whitespace error;
- static assertions cho batching, debounce, incremental hash, rolling prefetch, transaction persistence và source-map remap;
- mô hình 7.000 file để kiểm tra số lần thao tác.

Không thể chạy `cargo test`/`cargo check` vì file upload chỉ chứa thư mục source, không có `Cargo.toml`, lockfile và toolchain Rust trong môi trường. Vì vậy cần chạy hai lệnh đó trong project đầy đủ trước khi build bản phát hành.

## File thay đổi

- `src_backend/job.rs`
- `src_backend/job/sequential.rs`
