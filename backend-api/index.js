// ============================================================
// 🔥 0xoLemon Multi-Tenant Backend - Firebase Optimizer
// ============================================================
// Supports multiple Firebase projects (0xoLemon, 0xoLemon-1)
// Giải quyết: 807 listeners → 0, 49k reads/day → 100 reads/day per tenant

const express = require('express');
const cors = require('cors');
const NodeCache = require('node-cache');
const admin = require('firebase-admin');

const app = express();
const PORT = process.env.PORT || 8080;

// Cache: 1 giờ TTL per tenant
const cache = new NodeCache({ stdTTL: 3600 });

// ============================================================
// MULTI-TENANT FIREBASE ADMIN INIT
// ============================================================

/**
 * Tenant configuration
 * Each tenant has its own Firebase project and service account
 */
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
    projectId: 'xolemon-1',
    credentialsEnv: 'FIREBASE_0XOLEMON1_CREDENTIALS_JSON',
    app: null,
    db: null
  }
};

/**
 * Initialize Firebase Admin SDK for a specific tenant
 */
function initializeTenant(tenantId, config) {
  try {
    let serviceAccount;

    // Try to load credentials from env var
    const credentialsJson = process.env[config.credentialsEnv];
    if (credentialsJson) {
      serviceAccount = JSON.parse(credentialsJson);
      console.log(`✅ [${config.name}] Loaded credentials from ${config.credentialsEnv}`);
    } else {
      console.error(`❌ [${config.name}] Missing credentials: ${config.credentialsEnv}`);
      return false;
    }

    // Validate project ID matches
    if (serviceAccount.project_id !== config.projectId) {
      console.error(`❌ [${config.name}] Project ID mismatch! Expected: ${config.projectId}, Got: ${serviceAccount.project_id}`);
      return false;
    }

    // Initialize Firebase app with unique name
    config.app = admin.initializeApp({
      credential: admin.credential.cert(serviceAccount),
      projectId: config.projectId
    }, tenantId); // Use tenantId as app name

    config.db = config.app.firestore();

    console.log(`✅ [${config.name}] Firebase Admin initialized (Project: ${config.projectId})`);
    return true;
  } catch (error) {
    console.error(`❌ [${config.name}] Firebase init error:`, error.message);
    return false;
  }
}

// Initialize all tenants
console.log('🔥 Initializing multi-tenant Firebase...');
let initializedCount = 0;

for (const [tenantId, config] of Object.entries(TENANTS)) {
  if (initializeTenant(tenantId, config)) {
    initializedCount++;
  }
}

if (initializedCount === 0) {
  console.error('❌ No tenants initialized! Exiting...');
  process.exit(1);
}

console.log(`✅ Initialized ${initializedCount}/${Object.keys(TENANTS).length} tenants`);

/**
 * Get Firestore DB instance for a tenant
 */
function getTenantDb(tenantId) {
  const config = TENANTS[tenantId];
  if (!config || !config.db) {
    throw new Error(`Tenant '${tenantId}' not found or not initialized`);
  }
  return config.db;
}

// ============================================================
// MIDDLEWARE
// ============================================================
app.use(cors());
app.use(express.json());

// Tenant validator middleware
function validateTenant(req, res, next) {
  const tenantId = req.params.tenant;

  if (!tenantId) {
    return res.status(400).json({ error: 'Tenant ID required' });
  }

  if (!TENANTS[tenantId]) {
    return res.status(404).json({
      error: `Tenant '${tenantId}' not found`,
      availableTenants: Object.keys(TENANTS)
    });
  }

  if (!TENANTS[tenantId].db) {
    return res.status(503).json({
      error: `Tenant '${tenantId}' not initialized`,
      tenantName: TENANTS[tenantId].name
    });
  }

  next();
}

// Request logging with tenant info
app.use((req, res, next) => {
  const tenantId = req.params.tenant || 'N/A';
  console.log(`${new Date().toISOString()} - [${tenantId}] ${req.method} ${req.path}`);
  next();
});

// ============================================================
// HEALTH CHECK
// ============================================================
app.get('/health', (req, res) => {
  const tenantsStatus = {};
  for (const [id, config] of Object.entries(TENANTS)) {
    tenantsStatus[id] = {
      name: config.name,
      projectId: config.projectId,
      initialized: !!config.db
    };
  }

  res.json({
    status: 'ok',
    service: '0xoLemon Multi-Tenant Backend',
    version: '2.0.0',
    uptime: process.uptime(),
    cache_keys: cache.keys().length,
    tenants: tenantsStatus
  });
});

// ============================================================
// MULTI-TENANT API ENDPOINTS
// ============================================================

/**
 * GET /api/:tenant/catalog
 * Returns game catalog for specified tenant (cached 1 hour)
 */
app.get('/api/:tenant/catalog', validateTenant, async (req, res) => {
  try {
    const tenantId = req.params.tenant;
    const cacheKey = `${tenantId}:catalog`;
    let data = cache.get(cacheKey);

    if (!data) {
      console.log(`📡 [${tenantId}] Fetching catalog from Firestore...`);
      const db = getTenantDb(tenantId);
      const docRef = db.collection('config').doc('gameCatalog');
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Catalog not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log(`✅ [${tenantId}] Catalog cached`);
    } else {
      console.log(`💾 [${tenantId}] Serving catalog from cache`);
    }

    res.json({
      defaultLocale: data.defaultLocale || 'en-US',
      games: data.games || []
    });
  } catch (error) {
    console.error(`❌ [${req.params.tenant}] Error fetching catalog:`, error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/:tenant/assets
 * Returns assets override for specified tenant (cached 1 hour)
 */
app.get('/api/:tenant/assets', validateTenant, async (req, res) => {
  try {
    const tenantId = req.params.tenant;
    const cacheKey = `${tenantId}:assets`;
    let data = cache.get(cacheKey);

    if (!data) {
      console.log(`📡 [${tenantId}] Fetching assets from Firestore...`);
      const db = getTenantDb(tenantId);
      const docRef = db.collection('config').doc('assets_override');
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Assets not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log(`✅ [${tenantId}] Assets cached`);
    } else {
      console.log(`💾 [${tenantId}] Serving assets from cache`);
    }

    res.json(data);
  } catch (error) {
    console.error(`❌ [${req.params.tenant}] Error fetching assets:`, error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/:tenant/tags
 * Returns version tags for specified tenant (cached 1 hour)
 */
app.get('/api/:tenant/tags', validateTenant, async (req, res) => {
  try {
    const tenantId = req.params.tenant;
    const cacheKey = `${tenantId}:tags`;
    let data = cache.get(cacheKey);

    if (!data) {
      console.log(`📡 [${tenantId}] Fetching version tags from Firestore...`);
      const db = getTenantDb(tenantId);
      const docRef = db.collection('config').doc('version_tags');
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Version tags not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log(`✅ [${tenantId}] Version tags cached`);
    } else {
      console.log(`💾 [${tenantId}] Serving version tags from cache`);
    }

    res.json(data);
  } catch (error) {
    console.error(`❌ [${req.params.tenant}] Error fetching version tags:`, error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/:tenant/game-tags
 * Returns game tags for specified tenant
 */
app.get('/api/:tenant/game-tags', validateTenant, async (req, res) => {
  try {
    const tenantId = req.params.tenant;
    const cacheKey = `${tenantId}:game-tags`;
    let data = cache.get(cacheKey);

    if (!data) {
      console.log(`📡 [${tenantId}] Fetching game tags from Firestore...`);
      const db = getTenantDb(tenantId);
      const docRef = db.collection('config').doc('gameTags');
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Game tags not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log(`✅ [${tenantId}] Game tags cached`);
    } else {
      console.log(`💾 [${tenantId}] Serving game tags from cache`);
    }

    res.json(data);
  } catch (error) {
    console.error(`❌ [${req.params.tenant}] Error fetching game tags:`, error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/:tenant/game-stats
 * Returns game stats for specified tenant
 */
app.get('/api/:tenant/game-stats', validateTenant, async (req, res) => {
  try {
    const tenantId = req.params.tenant;
    const cacheKey = `${tenantId}:game-stats`;
    let data = cache.get(cacheKey);

    if (!data) {
      console.log(`📡 [${tenantId}] Fetching game stats from Firestore...`);
      const db = getTenantDb(tenantId);
      const docRef = db.collection('config').doc('gameStats');
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Game stats not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log(`✅ [${tenantId}] Game stats cached`);
    } else {
      console.log(`💾 [${tenantId}] Serving game stats from cache`);
    }

    res.json(data);
  } catch (error) {
    console.error(`❌ [${req.params.tenant}] Error fetching game stats:`, error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/:tenant/steam-appids
 * Returns Steam AppIDs mapping for specified tenant
 */
app.get('/api/:tenant/steam-appids', validateTenant, async (req, res) => {
  try {
    const tenantId = req.params.tenant;
    const cacheKey = `${tenantId}:steam-appids`;
    let data = cache.get(cacheKey);

    if (!data) {
      console.log(`📡 [${tenantId}] Fetching Steam AppIDs from Firestore...`);
      const db = getTenantDb(tenantId);
      const docRef = db.collection('config').doc('steam_appids');
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Steam AppIDs not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log(`✅ [${tenantId}] Steam AppIDs cached`);
    } else {
      console.log(`💾 [${tenantId}] Serving Steam AppIDs from cache`);
    }

    res.json(data);
  } catch (error) {
    console.error(`❌ [${req.params.tenant}] Error fetching Steam AppIDs:`, error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * GET /api/:tenant/game-details/:gameId
 * Returns detailed game metadata for specified tenant
 */
app.get('/api/:tenant/game-details/:gameId', validateTenant, async (req, res) => {
  try {
    const tenantId = req.params.tenant;
    const { gameId } = req.params;
    const cacheKey = `${tenantId}:game-details-${gameId}`;
    let data = cache.get(cacheKey);

    if (!data) {
      console.log(`📡 [${tenantId}] Fetching game details for ${gameId} from Firestore...`);
      const db = getTenantDb(tenantId);
      const docRef = db.collection('gameDetails').doc(gameId);
      const doc = await docRef.get();

      if (!doc.exists) {
        return res.status(404).json({ error: 'Game details not found' });
      }

      data = doc.data();
      cache.set(cacheKey, data);
      console.log(`✅ [${tenantId}] Game details for ${gameId} cached`);
    } else {
      console.log(`💾 [${tenantId}] Serving game details for ${gameId} from cache`);
    }

    res.json(data);
  } catch (error) {
    console.error(`❌ [${req.params.tenant}] Error fetching game details for ${req.params.gameId}:`, error);
    res.status(500).json({ error: error.message });
  }
});

/**
 * POST /api/:tenant/cache/clear
 * Clear cache for specified tenant (admin endpoint)
 */
app.post('/api/:tenant/cache/clear', validateTenant, (req, res) => {
  const tenantId = req.params.tenant;
  const { key } = req.body;

  if (key) {
    const tenantKey = `${tenantId}:${key}`;
    cache.del(tenantKey);
    console.log(`🗑️  [${tenantId}] Cleared cache key: ${key}`);
    res.json({ message: `Cache cleared for ${tenantId}:${key}` });
  } else {
    // Clear all keys for this tenant
    const allKeys = cache.keys();
    const tenantKeys = allKeys.filter(k => k.startsWith(`${tenantId}:`));
    tenantKeys.forEach(k => cache.del(k));
    console.log(`🗑️  [${tenantId}] Cleared ${tenantKeys.length} cache keys`);
    res.json({ message: `All cache cleared for tenant: ${tenantId}`, cleared: tenantKeys.length });
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
  console.log('🚀 0xoLemon Multi-Tenant Backend');
  console.log('========================================');
  console.log(`🌐 Server: http://0.0.0.0:${PORT}`);
  console.log(`📊 Health: http://0.0.0.0:${PORT}/health`);
  console.log('');
  console.log('🔥 Active Tenants:');
  for (const [id, config] of Object.entries(TENANTS)) {
    if (config.db) {
      console.log(`   ✅ ${config.name} (${id}) → ${config.projectId}`);
      console.log(`      API: /api/${id}/catalog, /api/${id}/assets, etc.`);
    } else {
      console.log(`   ❌ ${config.name} (${id}) → Not initialized`);
    }
  }
  console.log('========================================');
  console.log('');
});
