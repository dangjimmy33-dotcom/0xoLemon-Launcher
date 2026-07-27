# 🚀 Multi-Tenant Backend Setup Guide

## Step 1: Get Firebase Service Account Credentials

### For 0xoLemon (xolemon-b360e) - Already done ✅
You already have: `E:\xolemon-b360e-firebase-adminsdk-fbsvc-3f99099488.json`

### For 0xoLemon-1 (oxolemon1-d5da8) - Need to get ⚠️

1. **Go to Firebase Console:**
   ```
   https://console.firebase.google.com/project/oxolemon1-d5da8/settings/serviceaccounts/adminsdk
   ```

2. **Click "Generate new private key"**

3. **Save file** (e.g., `oxolemon1-d5da8-firebase-adminsdk.json`)

4. **IMPORTANT:** Keep this file secure! Never commit to git!

---

## Step 2: Prepare Credentials for Render

### Minify JSON (remove whitespace)

**Windows PowerShell:**
```powershell
# For 0xoLemon (already have)
$json = Get-Content "E:\xolemon-b360e-firebase-adminsdk-fbsvc-3f99099488.json" -Raw
$minified = $json -replace '\s+', ' '
$minified | Set-Clipboard
# Now paste into Render env var: FIREBASE_0XOLEMON_CREDENTIALS_JSON

# For 0xoLemon-1 (after downloading)
$json = Get-Content "path\to\oxolemon1-d5da8-firebase-adminsdk.json" -Raw
$minified = $json -replace '\s+', ' '
$minified | Set-Clipboard
# Now paste into Render env var: FIREBASE_0XOLEMON1_CREDENTIALS_JSON
```

**OR use online tool:**
1. Open: https://www.jsonformatter.io/json-minifier
2. Paste JSON content
3. Click "Minify"
4. Copy minified result

---

## Step 3: Set Environment Variables on Render

### Go to Render Dashboard:
```
https://dashboard.render.com/web/srv-ctmcj2u8ii6s73buikag/env
```

### Add/Update these variables:

#### 1. FIREBASE_0XOLEMON_CREDENTIALS_JSON
```json
{"type":"service_account","project_id":"xolemon-b360e","private_key_id":"...","private_key":"-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----\n","client_email":"...","client_id":"...","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://oauth2.googleapis.com/token","auth_provider_x509_cert_url":"https://www.googleapis.com/oauth2/v1/certs","client_x509_cert_url":"..."}
```

#### 2. FIREBASE_0XOLEMON1_CREDENTIALS_JSON (NEW)
```json
{"type":"service_account","project_id":"oxolemon1-d5da8","private_key_id":"...","private_key":"-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----\n","client_email":"...","client_id":"...","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://oauth2.googleapis.com/token","auth_provider_x509_cert_url":"https://www.googleapis.com/oauth2/v1/certs","client_x509_cert_url":"..."}
```

**⚠️ IMPORTANT:**
- Keep `\n` in private_key (don't remove them!)
- Must be valid JSON (use validator if unsure)
- Click "Save Changes" after adding

---

## Step 4: Deploy Backend

### Push to GitHub (auto-deploys to Render):
```bash
cd E:\007Launcher
git push
```

### Monitor deployment:
```
https://dashboard.render.com/web/srv-ctmcj2u8ii6s73buikag/events
```

**Expected logs:**
```
🔥 Initializing multi-tenant Firebase...
✅ [0xoLemon] Loaded credentials from FIREBASE_0XOLEMON_CREDENTIALS_JSON
✅ [0xoLemon] Firebase Admin initialized (Project: xolemon-b360e)
✅ [0xoLemon-1] Loaded credentials from FIREBASE_0XOLEMON1_CREDENTIALS_JSON
✅ [0xoLemon-1] Firebase Admin initialized (Project: oxolemon1-d5da8)
✅ Initialized 2/2 tenants

🚀 0xoLemon Multi-Tenant Backend
🌐 Server: http://0.0.0.0:8080
🔥 Active Tenants:
   ✅ 0xoLemon (0xolemon) → xolemon-b360e
      API: /api/0xolemon/catalog, /api/0xolemon/assets, etc.
   ✅ 0xoLemon-1 (0xolemon1) → oxolemon1-d5da8
      API: /api/0xolemon1/catalog, /api/0xolemon1/assets, etc.
```

---

## Step 5: Test Multi-Tenant Backend

### Health Check:
```bash
curl https://zeroxolemon-launcher.onrender.com/health
```

Expected response:
```json
{
  "status": "ok",
  "service": "0xoLemon Multi-Tenant Backend",
  "version": "2.0.0",
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

### Test 0xoLemon (Production):
```bash
curl https://zeroxolemon-launcher.onrender.com/api/0xolemon/catalog
curl https://zeroxolemon-launcher.onrender.com/api/0xolemon/tags
```

### Test 0xoLemon-1:
```bash
curl https://zeroxolemon-launcher.onrender.com/api/0xolemon1/catalog
curl https://zeroxolemon-launcher.onrender.com/api/0xolemon1/tags
```

**If you get 404 "Version tags not found":**
- 0xoLemon-1 Firebase chưa có data
- Need to copy data structure from 0xoLemon
- See "Step 6: Copy Data Structure" below

---

## Step 6: Copy Data Structure to 0xoLemon-1

### Option A: Manual Copy (Firebase Console)

1. **Open 0xoLemon Firebase:**
   ```
   https://console.firebase.google.com/project/xolemon-b360e/firestore/databases/-default-/data
   ```

2. **Open 0xoLemon-1 Firebase:**
   ```
   https://console.firebase.google.com/project/oxolemon1-d5da8/firestore/databases/-default-/data
   ```

3. **Copy these collections/documents:**
   - `config/gameCatalog` → Copy to 0xoLemon-1
   - `config/assets_override` → Copy to 0xoLemon-1
   - `config/version_tags` → Copy to 0xoLemon-1
   - `config/gameTags` → Copy to 0xoLemon-1
   - `config/gameStats` → Copy to 0xoLemon-1
   - `config/steam_appids` → Copy to 0xoLemon-1
   - `gameDetails` collection → Copy all documents

### Option B: Script to Clone Data (Recommended)

Create `copy-firebase-data.js`:
```javascript
const admin = require('firebase-admin');

// Initialize source (0xoLemon)
const source = admin.initializeApp({
  credential: admin.credential.cert(require('./xolemon-b360e-credentials.json'))
}, 'source');
const sourceDb = source.firestore();

// Initialize target (0xoLemon-1)
const target = admin.initializeApp({
  credential: admin.credential.cert(require('./oxolemon1-d5da8-credentials.json'))
}, 'target');
const targetDb = target.firestore();

async function copyCollection(collectionName) {
  const snapshot = await sourceDb.collection(collectionName).get();
  const batch = targetDb.batch();
  
  snapshot.docs.forEach(doc => {
    const ref = targetDb.collection(collectionName).doc(doc.id);
    batch.set(ref, doc.data());
  });
  
  await batch.commit();
  console.log(`✅ Copied ${snapshot.size} documents from ${collectionName}`);
}

async function main() {
  // Copy config documents
  const configDocs = ['gameCatalog', 'assets_override', 'version_tags', 'gameTags', 'gameStats', 'steam_appids'];
  
  for (const docId of configDocs) {
    const doc = await sourceDb.collection('config').doc(docId).get();
    if (doc.exists) {
      await targetDb.collection('config').doc(docId).set(doc.data());
      console.log(`✅ Copied config/${docId}`);
    }
  }
  
  // Copy gameDetails collection
  await copyCollection('gameDetails');
  
  console.log('🎉 Data copy complete!');
}

main().catch(console.error);
```

Run:
```bash
node copy-firebase-data.js
```

---

## Step 7: Verify Data in 0xoLemon-1

```bash
# Should return game catalog
curl https://zeroxolemon-launcher.onrender.com/api/0xolemon1/catalog

# Should return version tags
curl https://zeroxolemon-launcher.onrender.com/api/0xolemon1/tags

# Should return assets
curl https://zeroxolemon-launcher.onrender.com/api/0xolemon1/assets
```

---

## Troubleshooting

### Error: "Tenant not initialized"
**Cause:** Missing or invalid credentials env var

**Fix:**
1. Check Render env vars are set correctly
2. Verify JSON is valid (use https://jsonlint.com)
3. Check Render logs for error messages
4. Redeploy after fixing env vars

### Error: "Project ID mismatch"
**Cause:** Credentials JSON has wrong project_id

**Fix:**
1. Verify credentials are for correct Firebase project
2. Check `project_id` in JSON matches `TENANTS` config
3. Download fresh credentials from Firebase Console

### Error: "Version tags not found"
**Cause:** 0xoLemon-1 Firebase doesn't have data yet

**Fix:**
1. Copy data from 0xoLemon (see Step 6)
2. Or manually create documents in Firebase Console

### Backend not auto-deploying
**Cause:** GitHub push didn't trigger Render

**Fix:**
1. Check Render dashboard → Deploy → Latest deploy
2. Manual deploy: Click "Manual Deploy" → "Deploy latest commit"
3. Check GitHub webhook is connected

---

## Next Steps

After backend is working with both tenants:

1. **Update Launcher** to support tenant switching
2. **Add tenant selector UI** (Settings page)
3. **Test both tenants** in launcher
4. **Monitor Firebase quotas** for both projects

See `LAUNCHER_INTEGRATION.md` for frontend integration guide.
