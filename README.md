# 007 First Light Smart Launcher

Tauri v2 + React launcher for a Steam-like update pipeline:

- FastCDC content-defined chunks targeting 1 MiB.
- 256 MiB transport packs for low request count over a free proxy.
- Manifest-driven scan, verify, repair, rollback, cache, and resumable jobs.
- No HDiffPatch runtime dependency.
- No Hugging Face URL or token in the launcher.

---

## Run the launcher (dev mode)

```powershell
cd E:\007Launcher
npm install
npm run tauri:dev
```

## Build the launcher (production)

```powershell
cd E:\007Launcher\src-tauri
cargo tauri build
```

---

## Publish a new game version

> **TL;DR**: mỗi lần có bản game mới, chỉ cần 3 bước:
> 1. Set token trong shell.
> 2. Chạy `build-version --extend-existing`.
> 3. Cập nhật `fallbackCatalog` trong `src/App.tsx`.

### Bước 1 — Biên dịch depot builder (chỉ cần làm lại nếu code thay đổi)

```powershell
cd E:\007Launcher\src-tauri
cargo build --release --bin depot_builder
```

### Bước 2 — Set HF token (chỉ trong shell hiện tại)

```powershell
$env:HF_TOKEN = "hf_xxxxxxxxxxxxxxxxxxxx"
$env:HF_HUB_ENABLE_HF_TRANSFER = "1"    # tùy chọn, tăng tốc upload
```

### Bước 3 — Build và upload version mới

```powershell
cd E:\007Launcher\src-tauri

.\target\release\depot_builder.exe build-version `
  --input        "E:\<thư mục game mới>"         `
  --version      vX.Y                             `
  --out          "E:\007Launcher\depot\007-first-light" `
  --game-id      007-first-light                  `
  --launch-executable "Retail\007FirstLight.exe"  `
  --extend-existing                               `
  --upload-repo  "CatManga/Cat-Manga"             `
  --repo-type    dataset                          `
  --repo-prefix  "007-first-light"
```

**Ví dụ thực tế** — v1.3 với game nằm ở `E:\007 First Light`:

```powershell
.\target\release\depot_builder.exe build-version `
  --input        "E:\007 First Light"             `
  --version      v1.3                             `
  --out          "E:\007Launcher\depot\007-first-light" `
  --game-id      007-first-light                  `
  --launch-executable "Retail\007FirstLight.exe"  `
  --extend-existing                               `
  --upload-repo  "CatManga/Cat-Manga"             `
  --repo-type    dataset                          `
  --repo-prefix  "007-first-light"
```

> `--extend-existing` đọc catalog hiện tại, tái sử dụng chunk cũ, và bắt đầu đánh số pack từ sau pack cuối cùng đã có. Không ghi đè bất kỳ pack nào đã upload.

### Bước 4 — Cập nhật fallbackCatalog trong launcher

Mở `E:\007Launcher\src\App.tsx`, tìm `fallbackCatalog`, cập nhật:

```ts
latestVersion: 'vX.Y',
availableVersions: [
  // ... giữ nguyên các version cũ, đổi latest: false
  { version: 'vX.Y', label: 'Update X.Y', buildId: 'XXXXXXX', sizeBytes: 49_690_000_000, latest: true },
],
```

---

## Build depot từ đầu (build-pair — chỉ dùng lần đầu)

```powershell
cd E:\007Launcher\src-tauri
cargo run --bin depot_builder -- build-pair `
  --old-input "E:\007 First Light - Sao chép" `
  --new-input "E:\007 First Light"            `
  --out "E:\007Launcher\depot\007-first-light" `
  --old-version v1.0 --new-version v1.1       `
  --launch-executable "Retail\007FirstLight.exe" `
  --upload-repo "CatManga/Cat-Manga" --repo-type dataset --repo-prefix "007-first-light"
```

The builder streams large RPKG files; it does not load them fully into memory.

---

## Proxy Worker (Cloudflare)

```powershell
cd E:\007Launcher\worker
npm install
npm run types
npx wrangler secret put HF_ORIGIN_BASE
npx wrangler secret put HF_BEARER_TOKEN
npm run deploy
```

`HF_ORIGIN_BASE` should point to the repo prefix, for example:

```
https://huggingface.co/datasets/CatManga/Cat-Manga/resolve/main/007-first-light
```

`HF_BEARER_TOKEN` is optional if the origin is public.

---

## Safety

Cleanup, repair, and uninstall flows must operate from manifest-owned file lists. The project intentionally avoids recursive deletion commands and recursive cleanup flags.
