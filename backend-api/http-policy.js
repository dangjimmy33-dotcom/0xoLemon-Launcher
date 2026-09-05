const express = require('express');
const cors = require('cors');
const rateLimit = require('express-rate-limit');
const { randomUUID } = require('node:crypto');

// Desktop API access must not widen the cookie-authenticated web origin policy.
const DESKTOP_ORIGINS = new Set(['http://tauri.localhost', 'http://localhost:14201', 'http://localhost:1420']);
const PUBLIC_READ = /^\/api\/[^/]+\/(?:catalog|assets|tags|game-tags|game-stats|search-stats|app-settings|cloud-save-map|steam-appids|game-details\/[^/]+|lua-shop\/catalog\/search)\/?$/;
const NATIVE_API = /^\/api\/[^/]+\/(?:activation(?:\/|$)|lua-shop(?:\/|$)|social(?:\/|$)|devices\/register\/?$|devices\/[^/]+\/registration\/?$)/;

function createHttpPolicy(config, limits = {}) {
  const router = express.Router();
  router.use((req, res, next) => {
    req.requestId = randomUUID();
    res.setHeader('X-Request-Id', req.requestId);
    next();
  });
  router.use(cors((req, callback) => {
    const origin = req.get('origin');
    const webAllowed = origin && config.allowedOrigins.has(origin.replace(/\/$/, ''));
    const desktopAllowed = DESKTOP_ORIGINS.has(origin)
      && (req.path === '/health' || PUBLIC_READ.test(req.path) || NATIVE_API.test(req.path));
    if (origin && !webAllowed && !desktopAllowed) {
      const error = new Error('CORS_ORIGIN_DENIED');
      error.status = 403;
      callback(error);
      return;
    }
    callback(null, {
      origin: true,
      credentials: Boolean(webAllowed),
      exposedHeaders: ['Retry-After', 'RateLimit', 'RateLimit-Policy'],
      maxAge: 600
    });
  }));
  router.use((error, req, res, next) => {
    if (error.message !== 'CORS_ORIGIN_DENIED') return next(error);
    return res.status(403).json({ code: 'CORS_ORIGIN_DENIED', requestId: req.requestId });
  });

  // CORS rejection and preflight never consume application quotas. Catalog
  // reads have a separate budget; startup polling cannot exhaust write/auth APIs.
  const readLimiter = rateLimit({
    windowMs: 5 * 60 * 1000,
    limit: limits.read ?? 600,
    standardHeaders: 'draft-7',
    legacyHeaders: false,
    message: { error: 'CATALOG_RATE_LIMITED' }
  });
  const writeLimiter = rateLimit({
    windowMs: 15 * 60 * 1000,
    limit: limits.write ?? 100,
    standardHeaders: 'draft-7',
    legacyHeaders: false,
    message: { error: 'API_RATE_LIMITED' }
  });
  router.use((req, res, next) => {
    if ((req.method === 'GET' || req.method === 'HEAD') && req.path === '/health') return next();
    const limiter = (req.method === 'GET' || req.method === 'HEAD') && PUBLIC_READ.test(req.path)
      ? readLimiter : writeLimiter;
    return limiter(req, res, next);
  });
  return router;
}

module.exports = { createHttpPolicy };
