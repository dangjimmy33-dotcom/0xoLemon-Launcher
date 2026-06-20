# 0xoLemon Launcher 0.2.0 — Premium Home, Update Center và Notification System

## Tóm tắt

- Thêm Home dashboard theo hướng Xbox/GOG; Store và Library vẫn tách riêng.
- Thay banner updater bằng Update Center có tiến trình thật.
- Xây notification center dựa hoàn toàn trên event backend thật, hỗ trợ thông báo Windows.
- Thêm titlebar kiểu OS với đồng hồ, mạng, download và chuông.
- Chuẩn hóa animation bằng Motion for React, có carousel, tooltip động và onboarding.
- Discord mở bằng trình duyệt; ảnh donate hiển thị trong modal riêng.
- Mọi thành phần mới đều có tùy chọn tương ứng trong Settings.

## Thay đổi chính

### Home và bố cục

- Sidebar: Home, Store, Library, Updates, Downloads, Cloud Saves, Cache, Settings.
- Home mặc định cho cài đặt mới; người dùng hiện tại giữ startup page cũ.
- Home chỉ dùng dữ liệu thật:
  - Continue Playing từ `lastPlayedAt`.
  - Recent Games từ game đã cài.
  - Active Tasks từ job/update/cloud-save hiện tại.
  - Carousel từ game đã cài, game có update hoặc news metadata có sẵn.
- Carousel đổi slide mỗi 8 giây, dừng khi hover, focus, mất focus cửa sổ hoặc bật Reduce Motion.
- Discord mở `https://discord.gg/7ZXdTUVsJE`.
- Donate dùng `src/assets/donate/donate.png`, chỉ hiện trong card nhỏ và modal hỗ trợ.
- Cloud Saves có trang tổng quan; cấu hình chi tiết từng game vẫn nằm trong Library.

### Update Center chính xác

- Tách updater thành các phase:
  `checking → downloading → verifying → installing → restarting`, hoặc `failed`.
- Callback kết thúc download chỉ chuyển sang `verifying`; chỉ chuyển `installing` sau khi Tauri xác minh chữ ký thành công.
- Payload bổ sung version, phase, downloaded/total bytes, timestamp và lỗi.
- Frontend tính tốc độ bằng EWMA và ETA từ mẫu byte/thời gian; không hiển thị ETA nếu dữ liệu chưa đủ.
- Download có progress xác định; verify/install dùng trạng thái indeterminate vì Tauri không cung cấp phần trăm thật.
- Giao diện gồm:
  - Mini-progress luôn thấy trên titlebar.
  - Banner gọn khi phát hiện update.
  - Drawer Update Center với progress bar, byte, tốc độ, ETA, phase stepper, release notes và retry.
- “Update later” chỉ có trước khi bắt đầu. Trong lúc tải chỉ cho Hide; không tạo nút pause/cancel giả.

### Notification thật

- Thêm backend notification service với lịch sử tối đa 200 mục trong AppData.
- Interface chính:
  - `NotificationRecord`: id, category, severity, title, message, timestamp, read, dedupeKey, entity và action.
  - Commands: list, mark read/all read, clear và open action.
  - Event: `launcher://notification`.
- Nguồn thông báo:
  - Launcher update available/completed/failed.
  - Install, update, repair committed/failed/canceled.
  - Cloud-save conflict, restore và lỗi.
  - Uninstall, cache cleanup, temporary-data cleanup.
  - Achievement unlock và lỗi runtime quan trọng.
- Không tạo notification từ mỗi progress tick, game start/exit hoặc cloud-sync thành công nền theo mặc định.
- Deduplicate bằng event type + entity/job/version + terminal state.
- Toast trong app khi cửa sổ foreground; Windows notification khi launcher minimized/unfocused để tránh hiện hai lần.
- Windows notification dùng plugin chính thức của Tauri; bản dev tự fallback về in-app.
- Khi click notification, launcher focus và mở đúng tab/game/job liên quan.

### Motion, onboarding và OS status

- Thêm Motion for React và motion tokens dùng chung:
  - Micro interaction: 120–160 ms.
  - Panel/popover: spring stiffness 380, damping 34, mass 0.8.
  - Hero/shared-layout: spring stiffness 240, damping 30, mass 1.
- Chỉ animate `transform`, `opacity` và shared layout; không scroll hijacking.
- Scroll reveal tối đa 12 px và chỉ chạy lần đầu khi phần tử đi vào viewport.
- Tooltip hiện sau 650 ms hover; keyboard focus hiện ngay.
- Onboarding bốn bước ở lần mở đầu tiên, có Skip/Back/Next và Reset tutorial trong Settings.
- Titlebar status cluster gồm giờ/ngày hệ thống, mạng, active task và chuông notification.
- Popover dùng Acrylic blur; nền dài hạn dùng lớp Mica tối giả lập, có opaque fallback.
- Reduce Motion tắt autoplay, parallax, shared transition và animation trang nhưng giữ feedback cần thiết.

### Settings và migration

- Nâng preferences lên schema mới, migrate an toàn các giá trị hiện có.
- Home & Layout:
  - Startup page.
  - Hiện/ẩn Continue Playing, Recent Games, Active Tasks, Discord và Donate.
  - Carousel autoplay.
- Appearance:
  - Motion: Full, System hoặc Reduced.
  - Glass effects, scroll effects và hover hints.
- Status bar:
  - Hiện đồng hồ, ngày, network, download indicator và notification bell.
  - Định dạng giờ System/12h/24h.
- Notifications:
  - Master in-app, Windows notification, âm thanh và Do Not Disturb khi game chạy.
  - Toggle theo category: launcher, installs, downloads, cloud saves, storage, achievements và errors.
- Confirmations tách riêng:
  - Uninstall.
  - Cancel và xóa dữ liệu tải dở.
  - Clear cache.
  - Cloud restore/overwrite.
- Thêm Reset onboarding và quản lý notification history.
- Version trong About lấy động từ package/Tauri, không hardcode.
- Mặc định: in-app bật; Windows notification được hỏi trong onboarding; Do Not Disturb khi chơi game bật.

## Kiểm thử và nghiệm thu

- Update phase không nhảy sai; progress monotonic; không tạo phần trăm verify/install giả.
- Notification chỉ xuất hiện khi event thật xảy ra, không trùng sau reload hoặc event lặp.
- Native notification chỉ phát nền; category và master toggle hoạt động.
- Home không hiển thị game chưa cài trong Recent/Continue Playing.
- Carousel dừng khi hover/focus và tắt hoàn toàn khi Reduced Motion.
- Onboarding chỉ tự mở một lần và có thể chạy lại.
- Discord, donate và notification actions mở đúng đích.
- Keyboard navigation, focus trap, screen reader labels và độ tương phản được kiểm tra.
- Browser smoke-test tại 1120×720 và 1920×1080, không overflow hoặc console error.
- Chạy ESLint, TypeScript/Vite build, Rust tests, format check và signed NSIS/MSI build.
- Không publish GitHub Release tự động; build 0.2.0 được kiểm tra cục bộ trước.
- Không thay đổi vị trí `downloading/chunks/staging` và không dùng shell recursive deletion.

## Tài liệu tham chiếu

- [Windows Mica](https://learn.microsoft.com/en-us/windows/apps/design/style/mica)
- [Windows Acrylic](https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic)
- [Windows Motion](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/motion)
- [W3C Carousel accessibility](https://www.w3.org/WAI/tutorials/carousels/animations/)
- [Tauri Notifications](https://v2.tauri.app/plugin/notification/)
- [Motion for React](https://motion.dev/docs/react)
