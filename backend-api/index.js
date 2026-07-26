// ============================================================
// 🔥 0xoLemon Backend Middleware - Firebase Optimizer
// ============================================================
// Giải quyết: 807 listeners → 0, 49k reads/day → 100 reads/day

const express = require('express');
const cors = require('cors');
const NodeCache = require('node-cache');
const admin = require('firebase-admin');

const app = express();
const PORT = process.env.PORT || 8080;

// Cache: 1 giờ TTL
const cache = new NodeCache({ stdTTL: 3600 });

// ============================================================
// FIREBASE ADMIN INIT
// ============================================================
let serviceAccount;

// Method 1: Parse from GOOGLE_APPLICATION_CREDENTIALS_JSON env var (Render deployment)
if (process.env.GOOGLE_APPLICATION_CREDENTIALS_JSON) {
  try {
    serviceAccount = JSON.parse(process.env.GOOGLE_APPLICATION_CREDENTIALS_JSON);
    console.log('✅ Using GOOGLE_APPLICATION_CREDENTIALS_JSON');
  } catch (error) {
    console.error('❌ Failed to parse GOOGLE_APPLICATION_CREDENTIALS_JSON:', error.message);
    process.exit(1);
  }
}
// Method 2: Individual env vars (legacy support)
else if (process.env.FIREBASE_PRIVATE_KEY) {
  serviceAccount = {
    type: "service_account",
    project_id: process.env.FIREBASE_PROJECT_ID || "xolemon-b360e",
    private_key_id: process.env.FIREBASE_PRIVATE_KEY_ID,
    private_key: process.env.FIREBASE_PRIVATE_KEY.replace(/\\n/g, '\n'),
    client_email: process.env.FIREBASE_CLIENT_EMAIL,
    client_id: process.env.FIREBASE_CLIENT_ID,
    auth_uri: "https://accounts.google.com/o/oauth2/auth",
    token_uri: "https://oauth2.googleapis.com/token",
    auth_provider_x509_cert_url: "https://www.googleapis.com/oauth2/v1/certs",
    client_x509_cert_url: process.env.FIREBASE_CERT_URL
  };
  console.log('✅ Using individual Firebase env vars');
}
// Method 3: Fallback to Application Default Credentials
else {
  console.log('⚠️  No Firebase credentials found, using Application Default Credentials');
}

try {
  if (serviceAccount) {
    admin.initializeApp({
      credential: admin.credential.cert(serviceAccount)
    });
    console.log('✅ Firebase Admin initialized with service account');
  } else {
    admin.initializeApp();
    console.log('✅ Firebase Admin initialized with default credentials');
  }
} catch (error) {
  console.error('❌ Firebase init error:', error.message);
  process.exit(1);
}

const db = admin.firestore();

// ============================================================
// MIDDLEWARE
// ============================================================
app.use(cors());
app.use(express.json());

// Request logging
app.use((req, res, next) => {
  console.log(`${new Date().toISOString()} - ${req.method} ${req.path}`);
  next();
});

// ============================================================
// HEALTH CHECK
// ============================================================
app.get('/health', (req, res) => {
  res.json({
    status: 'ok',
    service: '0xoLemon Backend Middleware',
    version: '1.0.0',
    uptime: process.uptime(),
    cache_keys: cache.keys().length
  });
});

// ============================================================
// API ENDPOINTS
// ============================================================

/**
 * GET /api/catalog
 * Returns game catalog (cached 1 hour)
 */
app.get('/api/catalog', async (req, res) => {
  try {
    const cacheKey = 'catalog';
    let data = cache.get(cacheKey);

    if (!data) {
      console.log('📡 Fetching catalog from Firestore...');
      const docRef = db.collection('config').doc('gameCatalog');
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Catalog not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log('✅ Catalog cached');
    } else {
      console.log('💾 Serving catalog from cache');
    }

    res.json({
      defaultLocale: data.defaultLocale || 'en-US',
      games: data.games || []
    });
  } catch (error) {
    console.error('❌ Error fetching catalog:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/assets
 * Returns assets override (SteamGridDB URLs, cached 1 hour)
 */
app.get('/api/assets', async (req, res) => {
  try {
    const cacheKey = 'assets';
    let data = cache.get(cacheKey);

    if (!data) {
      console.log('📡 Fetching assets from Firestore...');
      const docRef = db.collection('config').doc('assets_override');
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Assets not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log('✅ Assets cached');
    } else {
      console.log('💾 Serving assets from cache');
    }

    res.json(data);
  } catch (error) {
    console.error('❌ Error fetching assets:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/tags
 * Returns version tags (cached 1 hour)
 */
app.get('/api/tags', async (req, res) => {
  try {
    const cacheKey = 'tags';
    let data = cache.get(cacheKey);

    if (!data) {
      console.log('📡 Fetching version tags from Firestore...');
      const docRef = db.collection('config').doc('version_tags');
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Version tags not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log('✅ Version tags cached');
    } else {
      console.log('💾 Serving version tags from cache');
    }

    res.json(data);
  } catch (error) {
    console.error('❌ Error fetching version tags:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/game-tags
 * Returns game tags for filtering
 */
app.get('/api/game-tags', async (req, res) => {
  try {
    const cacheKey = 'game-tags';
    let data = cache.get(cacheKey);

    if (!data) {
      console.log('📡 Fetching game tags from Firestore...');
      const docRef = db.collection('config').doc('gameTags');
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Game tags not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log('✅ Game tags cached');
    } else {
      console.log('💾 Serving game tags from cache');
    }

    res.json(data);
  } catch (error) {
    console.error('❌ Error fetching game tags:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/game-stats
 * Returns game stats (cached 1 hour)
 */
app.get('/api/game-stats', async (req, res) => {
  try {
    const cacheKey = 'game-stats';
    let data = cache.get(cacheKey);

    if (!data) {
      console.log('📡 Fetching game stats from Firestore...');
      const docRef = db.collection('config').doc('gameStats');
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Game stats not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log('✅ Game stats cached');
    } else {
      console.log('💾 Serving game stats from cache');
    }

    res.json(data);
  } catch (error) {
    console.error('❌ Error fetching game stats:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/steam-appids
 * Returns Steam AppIDs mapping (cached 1 hour)
 */
app.get('/api/steam-appids', async (req, res) => {
  try {
    const cacheKey = 'steam-appids';
    let data = cache.get(cacheKey);

    if (!data) {
      console.log('📡 Fetching Steam AppIDs from Firestore...');
      const docRef = db.collection('config').doc('steam_appids');
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Steam AppIDs not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log('✅ Steam AppIDs cached');
    } else {
      console.log('💾 Serving Steam AppIDs from cache');
    }

    res.json(data);
  } catch (error) {
    console.error('❌ Error fetching Steam AppIDs:', error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/game-details/:gameId
 * Returns detailed game metadata (cached 1 hour)
 */
app.get('/api/game-details/:gameId', async (req, res) => {
  try {
    const { gameId } = req.params;
    const cacheKey = `game-details-${gameId}`;
    let data = cache.get(cacheKey);

    if (!data) {
      console.log(`📡 Fetching game details for ${gameId} from Firestore...`);
      const docRef = db.collection('gameDetails').doc(gameId);
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Game details not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log(`✅ Game details for ${gameId} cached`);
    } else {
      console.log(`💾 Serving game details for ${gameId} from cache`);
    }

    res.json(data);
  } catch (error) {
    console.error(`❌ Error fetching game details for ${req.params.gameId}:`, error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/cache/clear
 * Clear cache manually (admin endpoint)
 */
app.post('/api/cache/clear', (req, res) => {
  const { key } = req.body;

  if (key) {
    cache.del(key);
    console.log(`🗑️  Cleared cache key: ${key}`);
    res.json({ message: `Cache cleared for: ${key}` });
  } else {
    cache.flushAll();
    console.log('🗑️  Cleared all cache');
    res.json({ message: 'All cache cleared' });
  }
});

// ============================================================
// ERROR HANDLING
// ============================================================
app.use((req, res) => {
  res.status(404).json({ error: 'Endpoint not found' });
});

app.use((err, req, res, next) => {
  console.error('❌ Server error:', err);
  res.status(500).json({ error: 'Internal server error' });
});

// ============================================================
// START SERVER
// ============================================================
app.listen(PORT, '0.0.0.0', () => {
  console.log('');
  console.log('========================================');
  console.log('🚀 0xoLemon Backend Middleware');
  console.log('========================================');
  console.log(`🌐 Server: http://0.0.0.0:${PORT}`);
  console.log(`📊 Health: http://0.0.0.0:${PORT}/health`);
  console.log(`🔥 Firebase: ${process.env.FIREBASE_PROJECT_ID || 'xolemon-b360e'}`);
  console.log('========================================');
  console.log('');
});
