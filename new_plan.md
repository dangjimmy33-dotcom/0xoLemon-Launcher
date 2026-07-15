Đây là prompt mình đã tổng hợp đầy đủ, có bối cảnh, vấn đề, mục tiêu và hướng triển khai kỹ thuật. Bạn có thể gửi nguyên văn cho AI (Claude Opus, Gemini 3, Codex, GPT...) để nhờ nó thiết kế hoặc sửa source.

---

# Prompt

Tôi muốn sửa **DepotDownloaderMod** để hỗ trợ **Steam-style delta update/downgrade**, thay vì luôn tải lại gần như toàn bộ game.

## Repository

DepotDownloaderMod:

[https://github.com/oureveryday/DepotDownloaderMod](https://github.com/oureveryday/DepotDownloaderMod?utm_source=chatgpt.com)

DepotDownloader (original):

---

## Hiện trạng

Tôi dùng DepotDownloaderMod với các tham số:

```bat
DepotDownloaderMod.exe ^
-app 3321460 ^
-depot 3321461 ^
-manifest 8775153285106940722 ^
-manifestfile 3321461_8775153285106940722.manifest ^
-depotkeys 3321460.key ^
-apptokens 3321460.token ^
-dir "SteamLibrary\steamapps\common\Crimson Desert" ^
-max-downloads 256 ^
-verify-all
```

Game đã được Steam cài sẵn trong:

```
SteamLibrary
    steamapps
        common
            Crimson Desert
```

Manifest là **manifest gốc của Steam** (~5.5MB), không phải manifest text.

DepotDownloaderMod vẫn quét SHA/checksum nhưng cuối cùng gần như download lại toàn bộ game.

---

## Vấn đề

Theo mình tìm hiểu:

Steam update được vì Steam có:

* appmanifest.acf
* depot manifest
* chunk database
* depot cache

Steam biết game hiện tại đang ở manifest nào, sau đó:

```
Old Manifest
      ↓
New Manifest
      ↓
Compare ChunkID
      ↓
Reuse Existing Chunks
      ↓
Download Missing Chunks
```

DepotDownloaderMod cũng có logic copy/reuse chunk (copyChunks, neededChunks...), nhưng chỉ hoạt động khi có **Old Manifest + New Manifest**.

Trong trường hợp của tôi:

```
Steam Installation
        +
Old Manifest
```

không có manifest của bản hiện tại.

Do đó chương trình không thể reuse chunk và download gần như toàn bộ.

Thông tin này phù hợp với kiến trúc của DepotDownloader và các thay đổi từ phiên bản 3.x, nơi manifest được lưu theo định dạng Steam. ([GitHub][1])

---

## Mục tiêu

Tôi muốn bổ sung một chế độ mới, ví dụ:

```bash
-steamdir "D:\SteamLibrary\steamapps\common\Crimson Desert"
```

hoặc

```bash
-reuse-existing
```

để DepotDownloaderMod có thể sử dụng dữ liệu đã tồn tại trong thư mục Steam.

---

## Ý tưởng

Thay vì yêu cầu phải có Old Manifest, hãy dùng chính dữ liệu hiện có trong thư mục game.

Quy trình mong muốn:

```
Steam Folder
      ↓
Read Steam Manifest
      ↓
Enumerate Files
      ↓
Read Existing Files
      ↓
Hash/Checksum Existing Chunks
      ↓
Compare With Target Manifest
      ↓
Reuse Matching Chunks
      ↓
Only Download Missing Chunks
```

Nếu manifest Steam chứa:

* File list
* Chunk list
* ChunkID
* Adler32
* SHA
* Offset
* Compressed size
* Uncompressed size

thì hoàn toàn có thể xác định chunk nào đã tồn tại mà không cần manifest hiện tại.

---

## Nếu không thể reuse chunk

Hãy đánh giá phương án khác:

* Parse manifest
* So sánh file hiện có
* Sinh filelist
* Chỉ download các file thực sự khác

để giảm dung lượng tải.

---

## Tôi muốn AI giúp

Không chỉ giải thích.

Tôi muốn AI:

1. Clone và đọc source DepotDownloaderMod.

2. Xác định:

* class nào parse manifest
* class nào download chunk
* class nào verify checksum
* class nào quyết định neededChunks
* class nào copy chunk

3. Thiết kế kiến trúc cho chế độ:

```
-steamdir
```

4. Chỉ rõ:

* file cần sửa
* method cần sửa
* đoạn code cần thêm

5. Nếu có hạn chế khiến ý tưởng này không khả thi, hãy giải thích chính xác lý do ở mức kiến trúc (ví dụ thiếu metadata, CDN protocol, manifest format...), không chỉ trả lời chung chung.

6. Nếu khả thi, hãy triển khai patch hoàn chỉnh, có thể build được, thay vì chỉ đưa pseudocode.

---

## Mục tiêu cuối cùng

Tôi muốn DepotDownloaderMod hoạt động gần giống Steam nhất:

```
Steam Installed Game
        ↓
Detect Existing Data
        ↓
Reuse Existing Chunks
        ↓
Only Download Changed Chunks
        ↓
Downgrade / Update
```

thay vì luôn tải lại toàn bộ game.

[1]: https://github.com/SteamRE/DepotDownloader/releases?utm_source=chatgpt.com "Releases · SteamRE/DepotDownloader · GitHub"
