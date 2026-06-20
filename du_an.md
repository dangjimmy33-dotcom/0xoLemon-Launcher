## Chốt hướng đi: xây **Downloader V2**, không thay toàn bộ depot

Không cần upload hàng nghìn chunk lên Hugging Face, không cần chuyển sang C++/ASM, cũng không cần bỏ hệ thống pack `.bin` hiện tại.

Kiến trúc đích nên là:

```text
Chunk logic khoảng 1 MiB
        ↓
Nén riêng từng chunk
        ↓
Gộp vào pack .bin lớn trên HF
        ↓
Launcher tải các HTTP range thích ứng
        ↓
Verify → decrypt → decompress ngay
        ↓
Ghi thẳng vào đúng offset của file staging
        ↓
Hash → atomic commit
```

Mục tiêu là chuyển từ:

```text
Download → lưu .part → tách .chunk → tải xong → assembly
```

sang:

```text
Download chính là quá trình cài đặt
```

SteamPipe cũng chia file thành chunk khoảng 1 MB, nén/mã hóa từng chunk, rồi phía client giải mã, giải nén và đặt chúng vào đúng vị trí trong file. ([Steamworks][1])

---

# 1. Hiện trạng launcher của bạn

Tôi kiểm tra `src-tauri(19).zip`, hiện tại:

| Thành phần       | Hiện trạng                             |
| ---------------- | -------------------------------------- |
| Logical chunk    | FastCDC: 512 KiB–2 MiB, mục tiêu 1 MiB |
| Nén              | Zstd level 10                          |
| Remote pack      | Mặc định khoảng 256 MiB                |
| Download workers | Mặc định 8                             |
| HTTP range task  | Mặc định 16 MiB                        |
| Checkpoint disk  | `sync_data()` sau mỗi khoảng 4 MiB     |
| Client storage   | `.part → .chunk → assembly`            |
| Assembly         | Chỉ chạy sau khi tải đủ chunk          |
| Multi-repository | Đã có priority và fallback cơ bản      |

Manifest hiện đã chứa:

```text
file_offset
pack_offset
compressed_size
uncompressed_size
```

Nghĩa là **đã đủ thông tin để ghi trực tiếp chunk vào đúng vị trí file game**. Không phải thay lại cơ chế FastCDC hay định dạng pack từ đầu.

Multi-repository fallback cũng đã tồn tại; hiện launcher chọn repo có `catalog.json` hợp lệ và ghi nhớ nguồn đó cho các request tiếp theo. 

---

# 2. Kiến trúc Downloader V2 hoàn chỉnh

```text
┌──────────────────────────────────────────────┐
│ Catalog / Manifest                          │
│ - file map                                  │
│ - logical chunks                            │
│ - pack offset                               │
│ - target file offset                        │
└───────────────────┬──────────────────────────┘
                    ↓
┌──────────────────────────────────────────────┐
│ Download Planner                            │
│ - install / update / repair                 │
│ - local chunk reuse                         │
│ - adaptive HTTP ranges                      │
│ - source selection                          │
└───────────────────┬──────────────────────────┘
                    ↓
┌──────────────────────────────────────────────┐
│ Network Workers                             │
│ - connection pool                           │
│ - HTTP Range                                │
│ - retry / rate-limit handling               │
└───────────────────┬──────────────────────────┘
                    ↓
          Bounded compressed queue
                    ↓
┌──────────────────────────────────────────────┐
│ Verify / Decode                             │
│ - compressed hash                           │
│ - decrypt                                   │
│ - Zstd / raw                                │
└───────────────────┬──────────────────────────┘
                    ↓
┌──────────────────────────────────────────────┐
│ Direct Staging Writer                       │
│ - write_at(file_offset)                     │
│ - durable bitmap journal                    │
│ - disk reservation                          │
└───────────────────┬──────────────────────────┘
                    ↓
┌──────────────────────────────────────────────┐
│ Verify / Commit                             │
│ - final file hash                           │
│ - sync once                                 │
│ - tmp → target                              │
│ - rollback on failure                       │
└──────────────────────────────────────────────┘
```

Remote vẫn là:

```text
packs/pack-00000.bin
packs/pack-00001.bin
manifests/version.json
catalog.json
```

Không tạo hàng nghìn file trên HF. Điều này cũng phù hợp với khuyến nghị của Hugging Face: giữ số file thấp, dưới 100.000 file mỗi repo, dưới 10.000 entry mỗi folder và chia file rất lớn xuống dưới mức khuyến nghị 200 GB. Pack 256 MiB của bạn nhỏ hơn rất nhiều các ngưỡng này. ([Hugging Face][2])

---

# 3. Giai đoạn 0 — Đo lường trước khi thay kiến trúc

Trước khi viết pipeline mới, thêm telemetry nội bộ:

```text
network_bytes_per_sec
network_ewma
disk_write_bytes_per_sec
disk_read_bytes_per_sec
decompress_bytes_per_sec
sync_latency_ms
queue_bytes
active_requests
HTTP status counts
retry count
429 count
source switches
RAM buffer bytes
```

Mỗi job cần log được:

```text
Time downloading
Time waiting for disk
Time decompressing
Time verifying
Time committing
Total HTTP requests
Downloaded compressed bytes
Written uncompressed bytes
Peak temporary disk usage
```

Không tối ưu dựa trên biểu đồ Task Manager. Cần biết chính xác lúc tốc độ tụt là do:

* HF/CDN.
* `sync_data()`.
* Defender/antivirus.
* SSD.
* CPU decompress/hash.
* Retry.
* Backpressure.
* UI tính tốc độ sai.

Giữ engine hiện tại là `engine_v1` và thêm feature flag:

```text
engine_v2_streaming = false
```

Trong thời gian phát triển, có thể quay về V1 ngay nếu V2 gặp lỗi.

---

# 4. Giai đoạn 1 — Sửa engine hiện tại trước

Đây là phần ít rủi ro nhất nhưng có thể giảm tụt tốc rõ nhất.

## 4.1 Bỏ `sync_data()` mỗi 4 MiB

Hiện tám worker có thể đồng loạt ép dữ liệu xuống ổ đĩa sau mỗi 4 MiB. Đây là ứng viên lớn gây biểu đồ răng cưa.

Thay bằng:

```text
Checkpoint sau 64 MiB
hoặc sau 1–2 giây
hoặc khi range hoàn thành
hoặc khi pause/cancel
```

Journal lưu hai giá trị:

```text
received_bytes = đã nhận từ mạng
durable_bytes  = đã sync an toàn xuống disk
```

Sau crash:

```text
truncate .part về durable_bytes
→ tải tiếp phần còn thiếu
```

Không mất khả năng resume, nhưng giảm rất nhiều flush đồng thời.

## 4.2 Tốc độ và ETA

Dùng hai cửa sổ khác nhau:

```text
Displayed speed: EWMA 3–5 giây
ETA:             EWMA 15–30 giây
```

Không cộng assembly byte vào network speed.

UI nên hiện nguyên nhân:

```text
Downloading
Waiting for disk
Decompressing
Verifying
Finalizing
Rate limited — retrying
```

## 4.3 Tách network worker khỏi xử lý file

Network worker chỉ làm:

```text
request → stream response → queue
```

Không để chính worker đó tiếp tục:

```text
đọc .part
→ tạo nhiều .chunk
→ rename
→ hash
```

Việc xử lý dữ liệu phải chạy ở decode/writer worker riêng.

---

# 5. Giai đoạn 2 — Adaptive HTTP Range

**Chunk vẫn khoảng 1 MiB.** Chỉ kích thước chuyến tải thay đổi.

## Fresh install

Nếu cần trên 85–90% một pack:

```text
Range: 64–256 MiB
hoặc tải toàn pack bằng một request streaming
```

Không cần tạo `.bin` hoàn chỉnh trên ổ.

## Update

Chỉ gom các chunk gần nhau:

```text
Range: 8–32 MiB
Max wasted bytes: khoảng 5–10%
```

Nếu khoảng trống giữa hai chunk quá lớn thì tách request.

## Repair

Dùng range nhỏ:

```text
1–8 MiB
```

vì repair thường chỉ thiếu vài chunk.

Như vậy:

```text
Install:
ít request, throughput cao

Update:
không tải phần thừa

Repair:
tải rất chính xác
```

Không cố định tất cả trường hợp ở 16 MiB.

---

# 6. Giai đoạn 3 — Streaming direct-to-staging

Đây là thay đổi lớn nhất.

## Luồng mới

Khi response đang về:

```text
1. Nhận đủ transport bytes của chunk A
2. Kiểm tra compressed hash
3. Giải mã
4. Giải nén
5. Ghi chunk A vào file_offset
6. Đánh dấu durable khi checkpoint thành công
7. Tiếp tục chunk B
```

Không còn:

```text
range.part
→ hash.chunk
→ đọc hash.chunk
→ decompress
→ assembly
```

Có thể giữ spool file giới hạn nếu RAM/disk không theo kịp:

```text
RAM queue:   64–256 MiB
Disk spool:  256 MiB–1 GiB
```

Nhưng spool phải là **cửa sổ trượt**, không phải toàn bộ 74 GB dữ liệu tải.

## Journal

Mỗi chunk có state:

```text
0 = missing
1 = written but not durable
2 = durable and verified
```

Dùng bitmap/bitset, không cần một JSON object khổng lồ cho từng chunk.

Sau crash:

* State 2: giữ nguyên.
* State 1: kiểm tra lại hoặc tải lại.
* State 0: tải.

Các thay đổi resume/cancel hiện có đã cố giữ lại job lỗi hoặc job còn khả năng tiếp tục, đồng thời chỉ dọn staging sau khi commit thành công; nguyên tắc này phải được giữ trong V2. 

---

# 7. Giai đoạn 4 — Backpressure thực sự

Queue phải giới hạn theo **byte**, không theo số chunk.

Giá trị khởi đầu:

```text
Low watermark:   128–256 MiB
High watermark:  512 MiB–1 GiB
```

Logic:

```text
queue dưới low:
    cấp thêm request mới

queue ở vùng giữa:
    giữ nguyên concurrency

queue vượt high:
    dừng cấp task mới
    không hủy request đang chạy
```

Adaptive concurrency dùng AIMD:

```text
Ổn định, queue thấp:
    workers += 1

Queue đầy, disk latency cao, retry tăng:
    workers = max(1, workers / 2)
```

Không dùng công thức tuyến tính:

```text
workers = max_workers × disk_speed / network_speed
```

vì worker count và throughput không tỷ lệ tuyến tính.

---

# 8. Giai đoạn 5 — Quản lý dung lượng kiểu Steam

## Fresh install

Tạo file trong:

```text
downloading/<game>/files/
```

với đúng path và kích thước logic cuối cùng:

```text
downloading/Game/Data.pak
downloading/Game/Audio.pak
downloading/Game/Game.exe
```

Chunk được ghi thẳng vào offset tương ứng.

Dung lượng đỉnh nên gần:

```text
Installed size
+ bounded spool
+ journal
+ safety margin
```

Ví dụ game cài xong 90 GB không cần giữ thêm toàn bộ 74 GB nén.

## Disk reservation

Không chỉ tạo sparse file rồi hy vọng ổ vẫn còn chỗ.

Tạo:

```text
.downloading-reservation
```

Ban đầu giữ số dung lượng còn cần. Khi staging file thực sự được cấp phát thêm, reservation giảm tương ứng:

```text
Ban đầu:
staging allocated = 0 GB
reservation       = 92 GB

Giữa quá trình:
staging allocated = 45 GB
reservation       = 47 GB

Cuối:
staging allocated = 90 GB
reservation       = 2 GB
```

Như vậy tổng dung lượng chiếm giữ ổn định, tránh tải đến 95% rồi hết disk.

## Update

Commit theo từng file:

```text
A.tmp
→ lấy chunk cũ tại local
→ tải chunk mới
→ verify
→ A cũ thành A.bak
→ A.tmp thành A
→ xóa A.bak
```

Peak overhead gần:

```text
file lớn nhất đang rebuild
+ spool
```

không phải toàn bộ game cộng toàn bộ download.

Tuy nhiên, nếu game có một pack duy nhất 80 GB thì update vẫn có thể cần tạo file mới 80 GB. Steam cũng build phiên bản file mới song song với file cũ khi update và cảnh báo pack quá lớn sẽ gây nhiều I/O và yêu cầu thêm dung lượng. ([Steamworks][1])

UI cần hiện rõ:

```text
Download size:   74.2 GB
Installed size:  90.1 GB
Temporary space:  1.0 GB
Required free:   92.0 GB
```

---

# 9. Giai đoạn 6 — Compression Builder V2

Giữ:

```text
FastCDC target ≈ 1 MiB
Pack target ≈ 256 MiB
```

Không dùng solid 7z làm depot chính. Solid archive có thể nén sâu nhưng phá:

* Random access.
* Partial update.
* Chunk reuse.
* HTTP range chính xác.
* Repair.
* Streaming install.

## Codec theo chunk

Manifest V2 nên thêm:

```json
{
  "codec": "zstd"
}
```

hoặc:

```json
{
  "codec": "raw"
}
```

Builder:

```text
Nếu Zstd tiết kiệm đủ, ví dụ trên 2–3%:
    lưu Zstd

Nếu dữ liệu gần như không nén được:
    lưu raw
```

Không nén lại vô ích các loại:

```text
mp4
ogg
jpg
pak đã nén
ucas đã nén/mã hóa
zip/rar/7z
```

## Compression level thích ứng

Không tăng toàn bộ từ Zstd 10 lên 22.

Nên phân loại:

```text
Text/config/binary dễ nén:
    Zstd 15–19

Game asset thông thường:
    Zstd 10–15

Đã nén hoặc entropy cao:
    raw
```

Có thể sample vài MiB đầu file để chọn level, thay vì thử nhiều level cho mọi chunk.

Dictionary chỉ dùng cho nhóm nhỏ có cấu trúc lặp lại:

```text
JSON/config
localization
shader metadata
small binaries tương tự
```

Không kỳ vọng dictionary giúp video, audio hoặc texture nén.

Tỷ lệ `90 GB installed → 74 GB download` phụ thuộc dữ liệu game. Không có codec nào bảo đảm tỷ lệ đó. Steam đạt hiệu quả tổng thể nhờ nén chunk, chỉ tải depot cần thiết, dedupe và không lưu cả archive nén trên client. ([Steamworks][1])

---

# 10. Giai đoạn 7 — Rate limit Hugging Face

Đây phải là một phần của scheduler, không phải xử lý lỗi phụ.

## Không hard-code quota

Trang chính thức hiện hiển thị quota Resolver theo cửa sổ 5 phút:

```text
Anonymous, theo IP: 3.000
Free account:       5.000
PRO:               12.000
```

Nhưng Hugging Face nói quota Anonymous và Free có thể thay đổi theo tình trạng nền tảng. Vì vậy launcher phải đọc `RateLimit` và `RateLimit-Policy`, không đóng cứng các số trên vào code. ([Hugging Face][3])

## Dùng Resolver cho file

Pack, manifest và catalog nên tải bằng file resolver, không gọi API listing nhiều lần. Resolver có quota cao hơn API và được tối ưu cho tải file. ([Hugging Face][3])

## Không nhúng token của bạn vào launcher

Không được ship token chủ repo trong EXE.

Public game:

```text
anonymous resolver request
→ quota tính theo IP người dùng
```

Có thể hỗ trợ token cá nhân tùy chọn:

```text
Advanced settings → Hugging Face token
```

nhưng phải lưu trong Windows Credential Manager, không lưu plaintext trong JSON hoặc log.

## Global request limiter

Mỗi hostname có:

```text
Semaphore concurrency
Token bucket request rate
Circuit breaker
```

Ví dụ:

```text
huggingface.co
cdn-lfs...
cas-server...
```

không được để mỗi worker tự retry độc lập.

## Xử lý 429

Khi nhận 429:

```text
1. Đọc RateLimit header
2. Lấy số giây còn lại `t`
3. Dừng cấp request mới cho host đó
4. Đợi t + random jitter
5. Giảm concurrency một nửa
6. Thử lại một request probe
7. Tăng dần nếu ổn định
```

Không để 8–16 worker cùng retry sau đúng một khoảng thời gian, vì sẽ tạo “thundering herd”.

Hugging Face yêu cầu giảm request rate, backoff và retry đối với 429; các lỗi 500/503/504 cũng được coi là retryable. ([Hugging Face][4])

## Source circuit breaker

Multi-repository hiện tại dùng thứ tự ưu tiên cố định. Nâng thành:

```text
source score =
    latency
    throughput
    failure rate
    429 status
    5xx status
    recent health
```

Nếu source A lỗi liên tục:

```text
A OPEN trong 30–120 giây
→ chuyển source B
→ sau cooldown gửi probe
```

Không đổi source giữa chừng cho từng byte tùy tiện; chỉ đổi tại ranh giới chunk/range để vẫn xác minh được hash.

## Giảm request bằng coalescing

Fresh install cần range lớn vì:

```text
ít request
ít header
ít TLS/HTTP scheduling
ít khả năng đụng quota
throughput cao hơn
```

Update/repair dùng range nhỏ vì tổng số chunk cần tải vốn đã ít.

Đây là cách xử lý rate limit tốt hơn việc đơn giản giảm worker từ 8 xuống 2.

## Hugging Face trong hệ thống lâu dài

Hugging Face hiện dùng Xet làm backend cho file lớn, có chunk-level deduplication và được thiết kế để lưu binary lớn. ([Hugging Face][5])

Tuy vậy, launcher nên có abstraction:

```rust
trait DepotSource {
    fn catalog();
    fn manifest();
    fn fetch_range();
    fn health();
    fn rate_limit();
}
```

Để sau này cắm thêm:

```text
HF
Cloudflare R2
S3-compatible storage
CDN riêng
LAN cache
```

Đây là suy luận kiến trúc: HF có thể là nguồn chính ban đầu, nhưng không nên để downloader phụ thuộc cứng vào URL hoặc quota của một nhà cung cấp.

---

# 11. Chế độ hiệu năng cho người dùng

## Auto — mặc định

Tự điều chỉnh theo:

```text
disk type
disk latency
CPU load
RAM pressure
network throughput
queue occupancy
rate-limit state
```

## Maximum Speed

```text
range lớn
queue lớn
nhiều decode worker
ưu tiên tổng thời gian hoàn thành
```

## Low Impact

```text
ít worker
queue nhỏ
assembly concurrency thấp
tự giảm khi game đang chạy
```

Có thêm:

```text
Bandwidth limit
Pause while gaming
Disable downloads on metered network
Download schedule
Cache size limit
```

---

# 12. Ma trận kiểm thử đúng

RTX 4090 gần như chỉ ảnh hưởng render WebView/UI. Download engine cần ưu tiên kiểm thử:

| Thành phần | Ma trận                                              |
| ---------- | ---------------------------------------------------- |
| Storage    | HDD, SATA SSD, NVMe phổ thông, NVMe cao cấp          |
| CPU        | 4 core, 6 core, 8+ core                              |
| RAM        | 8 GB, 16 GB, 32 GB                                   |
| Network    | 30, 100, 500, 1.000 Mbps                             |
| OS         | Windows 10, Windows 11                               |
| Security   | Windows Defender mặc định, một số antivirus phổ biến |
| GPU/UI     | Intel iGPU, AMD, NVIDIA — smoke test giao diện       |

Test bắt buộc:

```text
Pause giữa HTTP range
Tắt launcher đột ngột
Mất điện giả lập
Disk full
File chunk hỏng
Manifest hỏng
429
503
Source đổi giữa job
Game đang chạy
Rollback
Repair
Update ngược phiên bản
```

---

# Thứ tự triển khai chốt

## P0 — làm ngay

```text
1. Thêm telemetry và stall reason
2. Bỏ sync_data mỗi 4 MiB
3. Durable offset journal
4. EWMA speed/ETA
5. Parse RateLimit headers
6. Global retry/backoff per host
7. Adaptive HTTP range
```

## P1 — cải tiến lớn

```text
8. Tách network / decode / writer
9. Byte-bounded queues
10. Direct write vào staging file
11. Chunk bitmap journal
12. Download và assembly đồng thời
13. Per-file verify + atomic commit
```

## P2 — dung lượng và độ bền

```text
14. File preallocation / reservation
15. Hiển thị download/install/temp/required size
16. Streaming spool giới hạn
17. Crash recovery ở mọi commit state
18. Codec zstd/raw thích ứng
```

## P3 — quy mô lớn

```text
19. Health-scored multi-source
20. Segmented persistent cache
21. Auto / Maximum / Low Impact
22. Worker process Rust riêng
23. CDN/source abstraction
24. Staged rollout + automatic fallback V1
```

## Không ưu tiên

```text
Custom ASM
Viết lại downloader bằng C++
Memory-mapped assembly ngay bây giờ
Upload mỗi chunk thành một file HF
7z solid làm depot
Tăng worker cố định để ép tốc độ
Giữ toàn bộ compressed download trên disk
```

**Plan cuối cùng:** giữ format depot Steam-like hiện có, tối ưu ngay `sync` và request scheduler, rồi xây pipeline streaming ghi thẳng vào file staging, thêm disk reservation và rate-aware multi-source. Đây là con đường đồng thời giải quyết tốc độ tụt, dung lượng tạm, thời gian assembly, khả năng resume và rate limit mà không phá tương thích với các depot đã phát hành.

[1]: https://partner.steamgames.com/doc/sdk/uploading "Uploading to Steam (Steamworks Documentation)"
[2]: https://huggingface.co/docs/hub/storage-limits "Storage limits · Hugging Face"
[3]: https://huggingface.co/docs/hub/rate-limits "Hub Rate limits · Hugging Face"
[4]: https://huggingface.co/docs/xet/api "CAS API Documentation · Hugging Face"
[5]: https://huggingface.co/docs/hub/xet/index "Xet: our Storage Backend · Hugging Face"
