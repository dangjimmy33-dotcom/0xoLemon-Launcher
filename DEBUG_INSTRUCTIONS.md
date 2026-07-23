# Debug Instructions - Black Screen After Discord Check

## Vấn đề
Sau khi qua màn hình "Checking your access", màn hình chính bị đen.

## Các bước debug:

### 1. Mở DevTools để xem console errors
```bash
# Chạy ứng dụng
npm run tauri dev
```

Sau khi màn hình đen, bấm **F12** hoặc **Ctrl+Shift+I** để mở DevTools và xem console.

### 2. Kiểm tra các điều kiện render trong App.tsx

Tìm các đoạn code liên quan đến:
- `showIntro` state
- `discordAuth.state === 'checking'`
- `isBlockedState`

### 3. Kiểm tra CSS

Xem file `App.css` và `premium.css` có selector nào khiến màn hình bị đen không:
- `background: black`
- `opacity: 0`
- `display: none`

### 4. Tạm thời disable Discord check

Trong `App.tsx`, tìm dòng:
```typescript
const initialDiscordAuthStatus: DiscordAuthStatus = {
  state: isTauriRuntime() ? 'checking' : 'notConfigured',
```

Thay đổi thành:
```typescript
const initialDiscordAuthStatus: DiscordAuthStatus = {
  state: 'authorized', // Tạm thời skip check
```

### 5. Kiểm tra showIntro state

Trong `App.tsx`, tìm:
```typescript
const [showIntro, setShowIntro] = useState(true)
```

Thay thành:
```typescript
const [showIntro, setShowIntro] = useState(false) // Tạm thời skip intro
```

## Các vấn đề có thể gặp:

1. **Hook dependency issue**: useEffect có dependency sai khiến infinite loop
2. **State update issue**: setState được gọi quá nhiều lần
3. **Conditional rendering bug**: Điều kiện render sai khiến không có component nào được hiển thị
4. **CSS z-index issue**: Một layer nào đó đè lên màn hình chính

## Cần kiểm tra thêm:

- File `src/components/DiscordAccessGate.tsx`
- File `src/components/IntroScreen.tsx`
- File `src/App.css` - tìm `.workspace`, `.main-content`
