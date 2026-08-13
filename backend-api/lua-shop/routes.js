const crypto = require('crypto');
const express = require('express');
const rateLimit = require('express-rate-limit');
const { ipKeyGenerator } = rateLimit;

const { loadActivationConfig } = require('../activation/config');
const { DiscordClient, bearerToken } = require('../activation/discord-client');
const { ActivationError } = require('../activation/errors');
const { searchSteamCatalog } = require('./catalog');
const { publishCommunityPackage } = require('./hf-publisher');
const { validateCanonicalPackage } = require('./package-validator');
const {
  FirestoreLuaShopQuota,
  LuaShopError,
  assertAppId,
  assertRequestId,
  assertTimezone
} = require('./quota');

const CONTRIBUTION_TTL_MS = 15 * 60 * 1000;

function accountHmacKey() {
  return String(process.env.LUA_SHOP_ACCOUNT_HMAC_KEY || '').trim();
}

function accountKeyFor(discordId) {
  const key = accountHmacKey();
  if (key.length < 32) {
    throw new LuaShopError(
      'LUA_SHOP_NOT_CONFIGURED',
      'Lua Shop account protection is not configured.',
      503
    );
  }
  return crypto.createHmac('sha256', key).update(String(discordId)).digest('hex');
}

function requestDocumentId(accountKey, requestId) {
  return crypto.createHash('sha256').update(`${accountKey}:${requestId}`).digest('hex');
}

function sendError(res, error) {
  const known = error instanceof LuaShopError || error instanceof ActivationError;
  if (!known) console.error('[lua-shop] internal failure:', error && error.message);
  res.status(known ? error.httpStatus : 500).json({
    code: known ? error.code : 'INTERNAL_ERROR',
    message: known ? error.message : 'Lua Shop could not complete this request.'
  });
}

function createLuaShopRouter({ getTenantDb }) {
  const router = express.Router({ mergeParams: true });
  const activationConfig = loadActivationConfig();
  const discord = new DiscordClient(activationConfig.discord);
  const quotaByTenant = new Map();

  const catalogLimiter = rateLimit({
    windowMs: 15 * 60 * 1000,
    max: 120,
    standardHeaders: true,
    legacyHeaders: false,
    message: { code: 'CATALOG_RATE_LIMITED', message: 'Too many catalog requests.' }
  });
  const accountIpLimiter = rateLimit({
    windowMs: 15 * 60 * 1000,
    max: 40,
    standardHeaders: true,
    legacyHeaders: false,
    keyGenerator: (req) => `${ipKeyGenerator(req.ip)}:${req.params.tenant || 'unknown'}`,
    message: { code: 'LUA_SHOP_RATE_LIMITED', message: 'Too many Lua Shop requests.' }
  });
  const accountIdentityLimiter = rateLimit({
    windowMs: 15 * 60 * 1000,
    max: 40,
    standardHeaders: true,
    legacyHeaders: false,
    keyGenerator: (req) => `${req.params.tenant || 'unknown'}:${req.luaShopAuth.accountKey}`,
    message: { code: 'LUA_SHOP_ACCOUNT_RATE_LIMITED', message: 'Too many Lua Shop requests for this account.' }
  });

  function quotaFor(tenantId) {
    if (!quotaByTenant.has(tenantId)) {
      quotaByTenant.set(tenantId, new FirestoreLuaShopQuota(getTenantDb(tenantId)));
    }
    return quotaByTenant.get(tenantId);
  }

  async function authenticate(req) {
    const user = await discord.authorize(bearerToken(req));
    return { accountKey: accountKeyFor(user.id) };
  }

  async function authenticateRequest(req, res, next) {
    try {
      req.luaShopAuth = await authenticate(req);
      next();
    } catch (error) {
      sendError(res, error);
    }
  }

  router.get('/catalog/search', catalogLimiter, async (req, res) => {
    try {
      res.json(await searchSteamCatalog({
        query: req.query.query,
        cursor: req.query.cursor,
        limit: req.query.limit
      }));
    } catch (error) {
      sendError(res, error);
    }
  });

  router.get('/quota', accountIpLimiter, authenticateRequest, accountIdentityLimiter, async (req, res) => {
    try {
      const { accountKey } = req.luaShopAuth;
      const timezone = req.query.timezone ? assertTimezone(req.query.timezone) : 'UTC';
      res.json(await quotaFor(req.params.tenant).status(accountKey, timezone));
    } catch (error) {
      sendError(res, error);
    }
  });

  router.post('/add/reserve', accountIpLimiter, authenticateRequest, accountIdentityLimiter, async (req, res) => {
    try {
      const { accountKey } = req.luaShopAuth;
      res.json(await quotaFor(req.params.tenant).reserve(accountKey, req.body || {}));
    } catch (error) {
      sendError(res, error);
    }
  });

  for (const [path, completed] of [['/add/complete', true], ['/add/fail', false]]) {
    router.post(path, accountIpLimiter, authenticateRequest, accountIdentityLimiter, async (req, res) => {
      try {
        const { accountKey } = req.luaShopAuth;
        res.json(await quotaFor(req.params.tenant).settle(accountKey, req.body || {}, completed));
      } catch (error) {
        sendError(res, error);
      }
    });
  }

  router.get('/community/:appid/status', catalogLimiter, async (req, res) => {
    try {
      const appid = assertAppId(req.params.appid);
      const repo = String(process.env.HF_LUA_COMMUNITY_REPO || 'Immaking/Luas').trim();
      const branch = String(process.env.HF_LUA_COMMUNITY_BRANCH || 'main').trim();
      const url = `https://huggingface.co/datasets/${repo}/resolve/${encodeURIComponent(branch)}/community/index/${appid}.json`;
      const response = await fetch(url, { headers: { 'Cache-Control': 'no-cache' } });
      if (response.status === 404) return res.json({ appid, available: false });
      if (!response.ok) throw new LuaShopError('COMMUNITY_STATUS_UNAVAILABLE', 'Community cache is unavailable.', 503);
      const index = await response.json();
      res.json({
        appid,
        available: true,
        revision: typeof index.latestRevision === 'string' ? index.latestRevision : null,
        updatedAt: typeof index.updatedAt === 'string' ? index.updatedAt : null
      });
    } catch (error) {
      sendError(res, error);
    }
  });

  router.post(
    '/community/contributions',
    accountIpLimiter,
    authenticateRequest,
    accountIdentityLimiter,
    express.raw({ type: 'application/zip', limit: '128mb' }),
    async (req, res) => {
      let requestRef = null;
      let claimed = false;
      try {
        const { accountKey } = req.luaShopAuth;
        const appid = assertAppId(req.get('x-app-id'));
        const requestId = assertRequestId(req.get('x-request-id'));
        const revision = String(req.get('x-revision') || '').trim().toLowerCase();
        if (!/^[a-f0-9]{64}$/.test(revision)) {
          throw new LuaShopError('INVALID_REVISION', 'Community revision must be a SHA-256 hash.');
        }
        if (!Buffer.isBuffer(req.body) || req.body.length === 0) {
          throw new LuaShopError('PACKAGE_REQUIRED', 'A ZIP package is required.');
        }

        const db = getTenantDb(req.params.tenant);
        requestRef = db.collection('luaShopContributions').doc(requestDocumentId(accountKey, requestId));
        const claim = await db.runTransaction(async (transaction) => {
          const snapshot = await transaction.get(requestRef);
          if (snapshot.exists) {
            const existing = snapshot.data();
            if (existing.accountKey !== accountKey || Number(existing.appid) !== appid || existing.revision !== revision) {
              throw new LuaShopError('IDEMPOTENCY_CONFLICT', 'requestId is already used for another contribution.', 409);
            }
            if (existing.status === 'completed') return { replay: true, result: existing.result };
            if (existing.status === 'processing' && Number(existing.expiresAtMs) > Date.now()) {
              throw new LuaShopError('CONTRIBUTION_IN_PROGRESS', 'This contribution is already being processed.', 409);
            }
          }
          transaction.set(requestRef, {
            accountKey,
            appid,
            revision,
            requestId,
            status: 'processing',
            updatedAtMs: Date.now(),
            expiresAtMs: Date.now() + CONTRIBUTION_TTL_MS
          });
          return { replay: false };
        });
        if (claim.replay) return res.json(claim.result);
        claimed = true;

        const packageInfo = await validateCanonicalPackage(req.body, appid, revision);
        const result = await publishCommunityPackage(req.body, packageInfo, accountKey, requestId);
        await requestRef.set({
          status: 'completed',
          result,
          completedAtMs: Date.now(),
          expiresAtMs: 0
        }, { merge: true });
        res.status(result.status === 'published' ? 201 : 200).json(result);
      } catch (error) {
        if (requestRef && claimed) {
          try {
            await requestRef.set({
              status: 'failed',
              errorCode: error && error.code || 'INTERNAL_ERROR',
              failedAtMs: Date.now(),
              expiresAtMs: 0
            }, { merge: true });
          } catch (writeError) {
            console.error('[lua-shop] contribution failure state could not be saved:', writeError.message);
          }
        }
        sendError(res, error);
      }
    }
  );

  return router;
}

module.exports = { accountKeyFor, createLuaShopRouter, requestDocumentId };
