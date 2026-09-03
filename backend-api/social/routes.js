const crypto = require('crypto');
const express = require('express');
const rateLimit = require('express-rate-limit');

const { DiscordClient, bearerToken } = require('../activation/discord-client');
const { loadSocialConfig } = require('./config');
const { SocialError, sendSocialError } = require('./errors');
const { SocialEventHub } = require('./event-hub');
const { HfCoverPublisher } = require('./hf-cover-publisher');
const { SocialService } = require('./service');

const AUTH_CACHE_MS = 60 * 1000;
const ACCOUNT_WINDOW_MS = 60 * 1000;
const ACCOUNT_MAX_REQUESTS = 120;

function tokenKey(token) {
  return crypto.createHash('sha256').update(token).digest('hex');
}

function createAccountLimiter(now = () => Date.now()) {
  const windows = new Map();
  return (identity) => {
    const currentMs = now();
    const current = windows.get(identity.id);
    if (!current || current.resetAtMs <= currentMs) {
      windows.set(identity.id, { count: 1, resetAtMs: currentMs + ACCOUNT_WINDOW_MS });
      return;
    }
    current.count += 1;
    if (current.count > ACCOUNT_MAX_REQUESTS) {
      throw new SocialError('SOCIAL_RATE_LIMITED', 'Too many social requests. Try again shortly.', 429, {
        retryAt: new Date(current.resetAtMs).toISOString()
      });
    }
  };
}

function createSocialRouter({ getTenantDb, config = loadSocialConfig(), eventHub = new SocialEventHub(), publisher = null, discordClient = null }) {
  const router = express.Router();
  const discord = discordClient || new DiscordClient(config.discord);
  const authCache = new Map();
  const limitAccount = createAccountLimiter();
  const coverPublisher = publisher || new HfCoverPublisher({ config: config.cover, getTenantDb, eventHub });
  const ipLimiter = rateLimit({
    windowMs: 5 * 60 * 1000,
    max: 300,
    standardHeaders: true,
    legacyHeaders: false,
    message: { code: 'SOCIAL_RATE_LIMITED', message: 'Too many social requests from this connection.' }
  });

  router.use(ipLimiter);
  router.use(async (req, res, next) => {
    try {
      const token = bearerToken(req);
      if (!token) throw new SocialError('AUTH_REQUIRED', 'Discord authorization is required.', 401);
      const key = tokenKey(token);
      const nowMs = Date.now();
      let cached = authCache.get(key);
      if (!cached || cached.expiresAtMs <= nowMs) {
        cached = { identity: await discord.authorizeProfile(token), expiresAtMs: nowMs + AUTH_CACHE_MS };
        authCache.set(key, cached);
      }
      req.socialIdentity = { ...cached.identity, tenantId: req.params.tenant };
      req.socialService = new SocialService(getTenantDb(req.params.tenant), config, eventHub);
      req.socialService.assertEnabled(req.socialIdentity);
      limitAccount(req.socialIdentity);
      await coverPublisher.hydrateTenant(req.params.tenant);
      next();
    } catch (error) {
      sendSocialError(res, error);
    }
  });

  const handle = (operation) => async (req, res) => {
    try {
      const result = await operation(req);
      if (!res.headersSent) res.json(result);
    } catch (error) {
      if (!res.headersSent) sendSocialError(res, error);
    }
  };

  router.get('/bootstrap', handle((req) => req.socialService.bootstrap(req.socialIdentity)));
  router.get('/users', handle((req) => req.socialService.search(
    req.socialIdentity,
    req.query.query,
    req.query.limit,
    req.query.cursor
  )));
  router.get('/users/:userId', handle((req) => req.socialService.getProfile(req.socialIdentity, req.params.userId)));
  router.patch('/profile', handle((req) => req.socialService.updateProfile(req.socialIdentity, req.body || {})));

  router.put(
    '/profile/cover',
    express.raw({ type: ['image/webp', 'application/octet-stream'], limit: config.cover.maxBytes }),
    handle(async (req) => {
      const profile = await req.socialService.ensureProfile(req.socialIdentity);
      const result = await coverPublisher.enqueue({
        tenantId: req.params.tenant,
        userId: req.socialIdentity.id,
        accountKey: req.socialService.accountKey(req.socialIdentity.id),
        buffer: req.body,
        expectedHash: String(req.get('x-cover-sha256') || '').toLowerCase()
      });
      return { ...result, previousPublishedHash: profile.coverHash || null };
    })
  );

  router.delete('/profile/cover', handle(async (req) => {
    const db = getTenantDb(req.params.tenant);
    const profileRef = db.collection('socialUsers').doc(req.socialIdentity.id);
    const profileSnapshot = await profileRef.get();
    const profile = profileSnapshot.exists ? profileSnapshot.data() : {};
    await profileRef.set({ coverHash: '', coverPath: '', coverRevision: (Number(profile.coverRevision) || 0) + 1, updatedAtMs: Date.now() }, { merge: true });
    if (profile.coverPath) {
      const cleanupId = crypto.createHash('sha256').update(`${req.params.tenant}:${profile.coverPath}`).digest('hex');
      await db.collection('socialCoverCleanup').doc(cleanupId).set({
        path: profile.coverPath,
        dueAtMs: Date.now() + config.cover.cleanupGraceMs,
        createdAtMs: Date.now()
      });
    }
    eventHub.publish(req.params.tenant, [], 'cover.removed', { userId: req.socialIdentity.id });
    return { removed: true };
  }));

  const relationshipRequestId = (req) => String(req.get('x-request-id') || '');
  router.post('/friend-requests', handle((req) => req.socialService.mutateRelationship(
    req.socialIdentity,
    req.body && req.body.targetUserId,
    'request',
    relationshipRequestId(req)
  )));
  router.post('/friend-requests/:userId/accept', handle((req) => req.socialService.mutateRelationship(
    req.socialIdentity,
    req.params.userId,
    'accept',
    relationshipRequestId(req)
  )));
  router.delete('/friend-requests/:userId', handle((req) => {
    const action = req.query.action === 'decline' ? 'decline' : 'cancel';
    return req.socialService.mutateRelationship(req.socialIdentity, req.params.userId, action, relationshipRequestId(req));
  }));
  router.delete('/friends/:userId', handle((req) => req.socialService.mutateRelationship(req.socialIdentity, req.params.userId, 'remove', relationshipRequestId(req))));
  router.put('/blocks/:userId', handle((req) => req.socialService.mutateRelationship(req.socialIdentity, req.params.userId, 'block', relationshipRequestId(req))));
  router.delete('/blocks/:userId', handle((req) => req.socialService.mutateRelationship(req.socialIdentity, req.params.userId, 'unblock', relationshipRequestId(req))));

  router.post('/presence', handle((req) => req.socialService.heartbeat(req.socialIdentity, req.body || {})));
  router.post('/stats/events', handle((req) => req.socialService.recordStatEvent(req.socialIdentity, req.body || {})));
  router.get('/leaderboard', handle((req) => req.socialService.leaderboard(req.socialIdentity, req.query)));
  router.put('/leaderboard/participation', handle((req) => req.socialService.setLeaderboardParticipation(req.socialIdentity, req.body && req.body.enabled)));

  router.get('/events', async (req, res) => {
    res.status(200);
    res.set({
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache, no-transform',
      Connection: 'keep-alive',
      'X-Accel-Buffering': 'no'
    });
    res.flushHeaders();
    const send = (event) => {
      res.write(`id: ${event.id}\n`);
      res.write(`event: ${event.type}\n`);
      res.write(`data: ${JSON.stringify({ id: event.id, type: event.type, createdAt: event.createdAt, payload: event.payload })}\n\n`);
    };
    const lastEventId = req.get('last-event-id') || req.query.lastEventId;
    if (lastEventId && !eventHub.canReplay(req.params.tenant, lastEventId)) {
      res.write(`event: social.resync\ndata: ${JSON.stringify({
        id: '',
        type: 'social.resync',
        createdAt: new Date().toISOString(),
        payload: { reason: 'event-history-unavailable' }
      })}\n\n`);
    }
    const unsubscribe = eventHub.subscribe({
      tenantId: req.params.tenant,
      userId: req.socialIdentity.id,
      lastEventId,
      send
    });
    res.write(`event: ready\ndata: ${JSON.stringify({ serverTime: new Date().toISOString() })}\n\n`);
    const heartbeat = setInterval(() => res.write(`: heartbeat ${Date.now()}\n\n`), 20 * 1000);
    const close = () => {
      clearInterval(heartbeat);
      unsubscribe();
    };
    req.on('close', close);
    req.on('aborted', close);
  });

  router.socialConfig = config;
  router.coverPublisher = coverPublisher;
  return router;
}

module.exports = { createAccountLimiter, createSocialRouter };
