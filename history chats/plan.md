0xoLemon Launcher — Kế hoạch nâng cấp toàn diện
Tổng quan
7 nhóm tính năng, ưu tiên theo mức độ ảnh hưởng thực tế.

Nhóm 1 — Sửa lỗi dung lượng hiển thị sai ✅ (Cao nhất)
Root cause
effectiveDownloadSize trong App.tsx dùng snapshot.updateSize làm nguồn ưu tiên, nhưng snapshot không reset khi chuyển game → giữ lại size cũ.

Proposed Changes
[MODIFY] 
App.tsx
Reset effectiveDownloadSize về 0 khi selectedGame?.id thay đổi bằng useMemo với dependency đúng.
Thêm useEffect watch selectedGame?.id → clear stale snapshot updateSize.
Nhóm 2 — Click title + Pull-to-refresh ✅
Proposed Changes
[MODIFY] 
CustomTitleBar.tsx
Wrap .titlebar-label thành <button> hoặc thêm onClick={() => location.reload()} với title "Click to reload launcher".
Thêm CSS hover effect nhẹ (underline + cursor pointer).
[NEW] 
usePullToRefresh.ts
Hook theo dõi wheel event ở đầu trang (scrollY === 0, deltaY < 0).
Sau 200ms giữ nguyên hướng cuộn lên → hiện animation "đang tải lại..." → gọi location.reload().
[MODIFY] 
App.css
Thêm CSS cho pull-to-refresh indicator: thanh gradient từ trên xuống, spin animation.
Nhóm 3 — Tương thích AMD + Local dependencies ⚠️
IMPORTANT

Phần này liên quan đến file binary (DLL, runtime). Chỉ làm phần có thể làm qua frontend/Tauri — cụ thể:

Proposed Changes
[MODIFY] 
App.tsx
Khi launch game, detect GPU vendor qua Tauri invoke('get_gpu_info') → nếu AMD → pass flag đặc biệt vào launch command.
[NEW] src-tauri/src/gpu_detect.rs (nếu chưa có)
Tauri command trả về GPU vendor string từ registry/WMI.
NOTE

Phần "copy DLL local fallback" cần biết rõ dependency nào bị thiếu trên AMD. Có thể làm sau khi user báo cụ thể game nào lỗi AMD.

Nhóm 4 — Tối ưu hiệu năng ✅
Proposed Changes
[MODIFY] 
library.tsx
Hiện tại LazyGameCardImage đã lazy load. Thêm React.memo cho GameCard component để tránh re-render không cần thiết.
Thêm useMemo cho filteredGames list.
[MODIFY] 
App.tsx
Wrap các callback lớn bằng useCallback nếu chưa có.
Giảm số lần re-render khi mergedVersionInfos tính toán.
Nhóm 5 — Tính năng cộng đồng + Game Turbo ⚠️
IMPORTANT

Online user count và Friend system yêu cầu backend mới (Firebase Presence / Firestore). Đây là tính năng lớn nhất trong danh sách.

Proposed Changes
[NEW] 
useOnlinePresence.ts
Dùng Firebase Realtime Database (free tier) để track presence.
Ghi users/{discordId}/lastSeen khi online, xóa khi offline.
Return onlineCount: number.
[MODIFY] 
CustomTitleBar.tsx
Hiện chip "🟢 {count} online" cạnh clock.
[NEW] Game Turbo — 
GameTurbo.ts
Tauri command: trước khi launch game → set process priority cao, tắt Windows GameBar, clear working set.
Nút "⚡ Turbo" trong UI launch dialog.
Nhóm 6 — Bảo mật cache & user data ⚠️
NOTE

Launcher hiện không lưu thông tin nhạy cảm trong plaintext đáng kể. Cache chính là response JSON từ Firebase/Render (không phải credential).

Phạm vi thực tế:
Preferences file (launcher_preferences.json) — có thể encrypt bằng AES-GCM với key từ machine ID.
Discord token — đã được Tauri Keychain lưu an toàn (không cần làm thêm nếu đã dùng tauri-plugin-store với encryption).
[NEW] 
crypto.rs
Helper encrypt/decrypt dùng aes-gcm crate cho preferences.
Nhóm 7 — Auto-retry khi mạng chập chờn ✅ (Dễ nhất)
Proposed Changes
[NEW] 
fetchWithRetry.ts
Utility fetchWithRetry(url, options, { maxRetries: 3, backoff: 'exponential' }).
Exponential backoff: 1s → 2s → 4s với jitter.
Detect network error vs HTTP error (chỉ retry network error + 5xx).
[MODIFY] Tất cả hooks có fetch:
useBackendVersionTags.ts — replace fetch(...) bằng fetchWithRetry(...).
useBackendCatalog.ts — same.
useBackendAssets.ts — same.
useBackendGameStats.ts — same.
Thứ tự triển khai đề xuất
#	Nhóm	Độ khó	Thời gian ước tính
1	Nhóm 7: Auto-retry	Dễ	20 phút
2	Nhóm 1: Fix size bug	Dễ	15 phút
3	Nhóm 2: Click reload + Pull-to-refresh	Trung bình	30 phút
4	Nhóm 4: Tối ưu hiệu năng	Trung bình	30 phút
5	Nhóm 5a: Online counter	Trung bình	45 phút
6	Nhóm 5b: Game Turbo	Khó (Rust)	60 phút
7	Nhóm 6: Encrypt cache	Khó (Rust)	60 phút
8	Nhóm 3: AMD compat	Cần info thêm	TBD
Open Questions
IMPORTANT

Nhóm 3 (AMD): Game nào đang bị lỗi AMD? Lỗi cụ thể là gì (crash, black screen, performance drop)? Cần biết để xác định DLL/runtime nào cần bundle.
Nhóm 5 (Online counter): Firebase project nào để lưu presence? Dùng xolemon-b360e (0xoLemon) hay project riêng? Có Realtime Database chưa hay chỉ có Firestore?
Nhóm 6 (Encryption): Launcher hiện lưu gì trong file local? Cần xem loadLauncherPreferences() để đánh giá mức độ nhạy cảm thực sự.
Game Turbo: Muốn tự động (bật khi launch) hay manual (user bật tắt)?