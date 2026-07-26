# 🚀 Deploy Backend lên Render

## Bước 1: Tạo Web Service trên Render

1. Vào https://dashboard.render.com/
2. Click **"New +" → "Web Service"**
3. Connect GitHub repo: `dangjimmy33-dotcom/0xoLemon-Launcher`

## Bước 2: Cấu hình Service

```
Name: oxolemon-backend
Region: Singapore (gần Việt Nam nhất)
Branch: main
Root Directory: backend-api
Runtime: Node
Build Command: npm install
Start Command: npm start
Instance Type: Free
```

## Bước 3: Environment Variables

Click **"Environment"** tab và thêm biến:

### Cách 1: Paste toàn bộ JSON (KHUYẾN NGHỊ)

```
GOOGLE_APPLICATION_CREDENTIALS_JSON = <paste nội dung file E:\xolemon-b360e-firebase-adminsdk-fbsvc-3f99099488.json>
```

**⚠️ LƯU Ý:** 
- Paste 1 dòng duy nhất, không xuống dòng!
- File JSON nằm ở `E:\xolemon-b360e-firebase-adminsdk-fbsvc-3f99099488.json`
- KHÔNG commit file này lên Git!

### Cách 2: Individual Fields (nếu cách 1 không work)

```
FIREBASE_PROJECT_ID = xolemon-b360e
FIREBASE_PRIVATE_KEY_ID = <lấy từ JSON file>
FIREBASE_PRIVATE_KEY = <lấy từ JSON file, giữ nguyên \n>
FIREBASE_CLIENT_EMAIL = <lấy từ JSON file>
FIREBASE_CLIENT_ID = <lấy từ JSON file>
FIREBASE_CERT_URL = <lấy từ JSON file>
```

## Bước 4: Deploy

Click **"Create Web Service"** và đợi deploy (~2-3 phút).

URL sẽ là: `https://oxolemon-backend.onrender.com`

## Bước 5: Test

```bash
curl https://oxolemon-backend.onrender.com/health
curl https://oxolemon-backend.onrender.com/api/catalog
```

## Bước 6: Setup UptimeRobot (giữ FREE tier không sleep)

1. Vào https://uptimerobot.com/
2. **Add New Monitor**:
   - Monitor Type: `HTTP(s)`
   - Friendly Name: `oxoLemon Backend`
   - URL: `https://oxolemon-backend.onrender.com/health`
   - Monitoring Interval: `5 minutes`
3. Click **Create Monitor**

## Bước 7: Update Launcher

File `.env` trong launcher đã được cấu hình:
```
VITE_BACKEND_URL=https://xoxolemon-launcher.onrender.com
```

Build lại launcher và phát hành bản update mới!

## 📊 Kết quả mong đợi

- **Trước:** 807 listeners + 49k reads/day
- **Sau:** 0 listeners + ~100 reads/day (chỉ backend gọi Firestore)
- **Tiết kiệm:** 99% quota!

## 🔒 Security Notes

- ⚠️ File JSON chứa private key **KHÔNG BAO GIỜ commit lên Git**
- ✅ Chỉ lưu trên Render Environment Variables
- ✅ Backend đã config CORS để chỉ accept request từ launcher domain
- ✅ Rate limiting được bật (100 requests/minute/IP)
