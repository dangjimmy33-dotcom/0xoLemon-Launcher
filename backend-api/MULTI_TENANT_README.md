# 🔥 Multi-Tenant Firebase Backend

## Overview

Backend hỗ trợ nhiều Firebase projects (tenants) với tách biệt rõ ràng, dễ quản lý.

**Active Tenants:**
- `0xolemon` → Firebase Project: `xolemon-b360e` (Production)
- `0xolemon1` → Firebase Project: `oxolemon1-d5da8` (0xoLemon-1)

---

## API Routes

### Multi-Tenant Format
```
/api/:tenant/:endpoint
```

**Examples:**
```bash
# 0xoLemon (Production)
GET /api/0xolemon/catalog
GET /api/0xolemon/assets
GET /api/0xolemon/tags
GET /api/0xolemon/game-details/007-first-light

# 0xoLemon-1
GET /api/0xolemon1/catalog
GET /api/0xolemon1/assets
GET /api/0xolemon1/tags
GET /api/0xolemon1/game-details/game-id
```

### Available Endpoints per Tenant
- `GET /api/:tenant/catalog` - Game catalog
- `GET /api/:tenant/assets` - Assets override (SteamGridDB URLs)
- `GET /api/:tenant/tags` - Version tags (cracked, clean file, việt hóa)
- `GET /api/:tenant/game-tags` - Game filter tags
- `GET /api/:tenant/game-stats` - Game statistics
- `GET /api/:tenant/steam-appids` - Steam AppID mappings
- `GET /api/:tenant/game-details/:gameId` - Detailed game metadata
- `POST /api/:tenant/cache/clear` - Clear cache for tenant

### Health Check
```bash
GET /health
```

Returns:
```json
{
  "status": "ok",
  "service": "0xoLemon Multi-Tenant Backend",
  "version": "2.0.0",
  "uptime": 123.456,
  "cache_keys": 10,
  "tenants": {
    "0xolemon": {
      "name": "0xoLemon",
      "projectId": "xolemon-b360e",
      "initialized": true
    },
    "0xolemon1": {
      "name": "0xoLemon-1",
      "projectId": "oxolemon1-d5da8",
      "initialized": true
    }
  }
}
```

---

## Environment Variables (Render Deployment)

### Required for each tenant:

**0xoLemon (Production):**
```bash
FIREBASE_0XOLEMON_CREDENTIALS_JSON={"type":"service_account","project_id":"xolemon-b360e",...}
```

**0xoLemon-1:**
```bash
FIREBASE_0XOLEMON1_CREDENTIALS_JSON={"type":"service_account","project_id":"oxolemon1-d5da8",...}
```

### How to get credentials JSON:

1. Go to Firebase Console → Project Settings
2. Select project (0xoLemon or 0xoLemon-1)
3. Service Accounts → Generate new private key
4. Copy entire JSON content
5. Minify JSON (remove newlines): `jq -c . < credentials.json`
6. Set in Render environment variables

---

## Adding a New Tenant

1. **Update `TENANTS` config in `index.js`:**
```javascript
const TENANTS = {
  '0xolemon': {
    name: '0xoLemon',
    projectId: 'xolemon-b360e',
    credentialsEnv: 'FIREBASE_0XOLEMON_CREDENTIALS_JSON',
    app: null,
    db: null
  },
  '0xolemon1': {
    name: '0xoLemon-1',
    projectId: 'oxolemon1-d5da8',
    credentialsEnv: 'FIREBASE_0XOLEMON1_CREDENTIALS_JSON',
    app: null,
    db: null
  },
  // Add new tenant here:
  'newtenant': {
    name: 'New Tenant Name',
    projectId: 'firebase-project-id',
    credentialsEnv: 'FIREBASE_NEWTENANT_CREDENTIALS_JSON',
    app: null,
    db: null
  }
};
```

2. **Set environment variable on Render:**
```bash
FIREBASE_NEWTENANT_CREDENTIALS_JSON={"type":"service_account",...}
```

3. **Deploy to Render** (auto-deploys on git push)

4. **Test new tenant:**
```bash
curl https://zeroxolemon-launcher.onrender.com/api/newtenant/catalog
```

---

## Cache Management

### Cache Keys Format
```
{tenantId}:{endpoint}
```

Examples:
- `0xolemon:catalog`
- `0xolemon1:assets`
- `0xolemon:game-details-007-first-light`

### Clear Cache
```bash
# Clear specific key for tenant
POST /api/0xolemon/cache/clear
Body: { "key": "catalog" }

# Clear all cache for tenant
POST /api/0xolemon1/cache/clear
Body: {}
```

---

## Testing

### Test all tenants:
```bash
# Health check
curl https://zeroxolemon-launcher.onrender.com/health

# 0xoLemon
curl https://zeroxolemon-launcher.onrender.com/api/0xolemon/catalog
curl https://zeroxolemon-launcher.onrender.com/api/0xolemon/tags

# 0xoLemon-1
curl https://zeroxolemon-launcher.onrender.com/api/0xolemon1/catalog
curl https://zeroxolemon-launcher.onrender.com/api/0xolemon1/tags
```

### Error cases:
```bash
# Invalid tenant
curl https://zeroxolemon-launcher.onrender.com/api/invalid/catalog
# Returns 404: Tenant 'invalid' not found

# Missing tenant
curl https://zeroxolemon-launcher.onrender.com/api//catalog
# Returns 400: Tenant ID required
```

---

## Tenant Isolation

✅ **Tách biệt hoàn toàn:**
- Mỗi tenant có Firebase App instance riêng
- Mỗi tenant có Firestore DB instance riêng
- Cache keys có prefix tenant ID
- Logs có label tenant ID
- Không có cross-tenant data leakage

✅ **Validation:**
- Middleware `validateTenant()` kiểm tra tenant ID hợp lệ
- Kiểm tra tenant đã initialized chưa
- Error messages rõ ràng khi tenant không tồn tại

✅ **Easy Management:**
- Thêm tenant mới chỉ cần update `TENANTS` config
- Set env var credentials
- Auto-initialize on server start
- Clear logs cho từng tenant

---

## Migration from Old API

**Old routes (deprecated):**
```
/api/catalog → /api/0xolemon/catalog
/api/assets → /api/0xolemon/assets
/api/tags → /api/0xolemon/tags
```

**Launcher cần update:**
1. Add tenant selector (UI hoặc config)
2. Pass tenant ID to backend hooks
3. Update `VITE_BACKEND_URL` to include tenant

---

## Production Checklist

- [ ] Set `FIREBASE_0XOLEMON_CREDENTIALS_JSON` on Render
- [ ] Set `FIREBASE_0XOLEMON1_CREDENTIALS_JSON` on Render
- [ ] Verify both tenants initialized in `/health` response
- [ ] Test catalog endpoint for both tenants
- [ ] Monitor Render logs for tenant labels
- [ ] Update launcher to pass tenant ID
- [ ] Test cache isolation between tenants

---

## Support

**Logs format:**
```
2026-07-26T12:34:56.789Z - [0xolemon] GET /api/0xolemon/catalog
📡 [0xolemon] Fetching catalog from Firestore...
✅ [0xolemon] Catalog cached

2026-07-26T12:35:10.123Z - [0xolemon1] GET /api/0xolemon1/assets
💾 [0xolemon1] Serving assets from cache
```

**Check tenant status:**
```bash
curl https://zeroxolemon-launcher.onrender.com/health | jq '.tenants'
```
