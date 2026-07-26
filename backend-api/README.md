# 🔥 0xoLemon Backend Middleware

Backend API to solve Firebase quota burn (807 listeners → 0, 49k reads → 100/day)

## 🚀 Deploy to Render

### 1. Push to GitHub
```bash
git add backend-api/
git commit -m "Add backend middleware"
git push origin main
```

### 2. Create Web Service on Render
- Go to: https://dashboard.render.com
- Click: **New → Web Service**
- Connect your GitHub repo
- **Settings:**
  - Name: `0xolemon-backend`
  - Root Directory: `backend-api`
  - Build Command: `npm install`
  - Start Command: `npm start`
  - Health Check Path: `/health`

### 3. Environment Variables
Add in Render dashboard:
```
FIREBASE_PROJECT_ID=xolemon-b360e
```

**Optional** (if using service account):
```
FIREBASE_PRIVATE_KEY_ID=<your-key-id>
FIREBASE_PRIVATE_KEY=<your-private-key>
FIREBASE_CLIENT_EMAIL=<firebase-adminsdk-email>
FIREBASE_CLIENT_ID=<client-id>
FIREBASE_CERT_URL=<cert-url>
```

### 4. Deploy!
Click **Create Web Service** → Wait 2-3 minutes

---

## 🎯 Local Testing

```bash
cd backend-api
npm install
npm start
```

Test endpoints:
```bash
curl http://localhost:8080/health
curl http://localhost:8080/api/catalog
curl http://localhost:8080/api/assets
curl http://localhost:8080/api/tags
```

---

## 📡 API Endpoints

- `GET /health` - Health check
- `GET /api/catalog` - Game catalog (cached 1h)
- `GET /api/assets` - Asset URLs (cached 1h)
- `GET /api/tags` - Version tags (cached 1h)
- `GET /api/game-tags` - Game filter tags (cached 1h)
- `POST /api/cache/clear` - Clear cache

---

## 📊 Performance

**Before:**
- 807 listeners
- 49,000 reads/day

**After:**
- 0 listeners
- ~100 reads/day

**Savings:** 99.8% ✅
