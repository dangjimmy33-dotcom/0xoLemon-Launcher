const express = require('express');
const rateLimit = require('express-rate-limit');
const crypto = require('crypto');
const { bearerToken } = require('../activation/discord-client');
const {
  appendSetCookie,
  cookie,
  hashToken,
  openJson,
  parseCookies,
  randomToken,
  requireSameOrigin,
  safeReturnPath,
  sealJson
} = require('./security');

function asyncRoute(handler) {
  return (req, res, next) => Promise.resolve(handler(req, res, next)).catch(next);
}

function createRemoteRouter({ service, config, eventHub }) {
  const router = express.Router();
  const authLimiter = rateLimit({ windowMs: 15 * 60 * 1000, max: 30, standardHeaders: true, legacyHeaders: false });
  const jobLimiter = rateLimit({ windowMs: 60 * 1000, max: 20, standardHeaders: true, legacyHeaders: false });

  const setCookie = (res, name, value, options = {}) => appendSetCookie(res, [cookie(name, value, {
    secure: config.secureCookies,
    ...options
  })]);

  const clearAuthCookies = (res) => appendSetCookie(res, [
    cookie(config.sessionCookieName, '', { maxAge: 0, httpOnly: true, secure: config.secureCookies }),
    cookie(config.csrfCookieName, '', { maxAge: 0, secure: config.secureCookies }),
    cookie(config.oauthCookieName, '', { maxAge: 0, httpOnly: true, secure: config.secureCookies })
  ]);

  const requireSession = asyncRoute(async (req, res, next) => {
    service.assertEnabled('web');
    const cookies = parseCookies(req.get('cookie'));
    const session = await service.getSession(req.params.tenant, cookies[config.sessionCookieName]);
    if (!session) return res.status(401).json({ error: 'WEB_SESSION_REQUIRED' });
    req.webSession = session;
    req.webSessionId = cookies[config.sessionCookieName];
    next();
  });

  const requireCsrf = (req, res, next) => {
    if (!requireSameOrigin(req, config)) return res.status(403).json({ error: 'ORIGIN_REJECTED' });
    const cookies = parseCookies(req.get('cookie'));
    const csrf = String(req.get('x-csrf-token') || '');
    if (!csrf || csrf !== cookies[config.csrfCookieName] || hashToken(csrf) !== req.webSession.csrfHash) {
      return res.status(403).json({ error: 'CSRF_REJECTED' });
    }
    next();
  };

  router.get('/auth/discord/start', authLimiter, asyncRoute(async (req, res) => {
    service.assertEnabled('web');
    const state = randomToken(32);
    const verifier = randomToken(48);
    const challenge = crypto.createHash('sha256').update(verifier).digest('base64url');
    const flow = sealJson({
      state,
      verifier,
      returnTo: safeReturnPath(req.query.returnTo),
      expiresAt: Date.now() + config.oauthTtlMs
    }, config.sessionKey);
    setCookie(res, config.oauthCookieName, flow, { httpOnly: true, maxAge: Math.floor(config.oauthTtlMs / 1000) });
    const url = new URL(config.discord.authorizeUrl);
    url.searchParams.set('client_id', config.discord.clientId);
    url.searchParams.set('redirect_uri', config.discord.redirectUri);
    url.searchParams.set('response_type', 'code');
    url.searchParams.set('scope', 'identify guilds guilds.members.read');
    url.searchParams.set('state', state);
    url.searchParams.set('code_challenge', challenge);
    url.searchParams.set('code_challenge_method', 'S256');
    url.searchParams.set('prompt', 'consent');
    res.redirect(302, url.toString());
  }));

  router.get('/auth/discord/callback', authLimiter, asyncRoute(async (req, res) => {
    service.assertEnabled('web');
    const cookies = parseCookies(req.get('cookie'));
    const flow = openJson(cookies[config.oauthCookieName], config.sessionKey);
    if (!flow || flow.expiresAt <= Date.now() || req.query.state !== flow.state || typeof req.query.code !== 'string') {
      clearAuthCookies(res);
      return res.redirect(302, `${config.publicWebUrl}/auth/error?code=OAUTH_STATE_INVALID`);
    }
    const grant = await service.exchangeDiscordCode({ code: req.query.code, verifier: flow.verifier });
    const session = await service.createWebSession(req.params.tenant, grant);
    appendSetCookie(res, [
      cookie(config.sessionCookieName, session.id, { httpOnly: true, secure: config.secureCookies, maxAge: Math.floor(config.sessionTtlMs / 1000) }),
      cookie(config.csrfCookieName, session.csrf, { secure: config.secureCookies, maxAge: Math.floor(config.sessionTtlMs / 1000) }),
      cookie(config.oauthCookieName, '', { httpOnly: true, secure: config.secureCookies, maxAge: 0 })
    ]);
    res.redirect(302, `${config.publicWebUrl}${safeReturnPath(flow.returnTo)}`);
  }));

  router.get('/auth/session', asyncRoute(async (req, res) => {
    if (!config.webAuthEnabled) return res.status(503).json({ enabled: false, authenticated: false });
    const cookies = parseCookies(req.get('cookie'));
    const session = await service.getSession(req.params.tenant, cookies[config.sessionCookieName]);
    if (!session) return res.json({ enabled: true, authenticated: false });
    const legal = await service.getLegalAcceptance(req.params.tenant, session.accountKey);
    res.json({ enabled: true, authenticated: true, user: session.profile, legal });
  }));

  router.post('/auth/logout', requireSession, requireCsrf, asyncRoute(async (req, res) => {
    await service.revokeSession(req.params.tenant, req.webSessionId);
    clearAuthCookies(res);
    res.status(204).end();
  }));

  router.get('/legal/acceptance', requireSession, asyncRoute(async (req, res) => {
    res.json(await service.getLegalAcceptance(req.params.tenant, req.webSession.accountKey));
  }));
  router.post('/legal/acceptance', requireSession, requireCsrf, asyncRoute(async (req, res) => {
    res.json(await service.acceptLegal(req.params.tenant, req.webSession.accountKey, req.body || {}));
  }));

  router.get('/web/catalog', requireSession, asyncRoute(async (req, res) => {
    res.json(await service.getWebCatalog(req.params.tenant));
  }));

  router.get('/devices', requireSession, asyncRoute(async (req, res) => {
    res.json({ devices: await service.listDevices(req.params.tenant, req.webSession.accountKey) });
  }));
  router.delete('/devices/:deviceId', requireSession, requireCsrf, asyncRoute(async (req, res) => {
    const revoked = await service.revokeDevice(req.params.tenant, req.webSession.accountKey, req.params.deviceId);
    res.status(revoked ? 204 : 404).end();
  }));

  router.post('/devices/register', authLimiter, asyncRoute(async (req, res) => {
    service.assertEnabled('remote');
    const account = await service.authorizeDesktopBearer(bearerToken(req));
    res.status(201).json(await service.registerDevice(req.params.tenant, account, req.body || {}));
  }));
  router.delete('/devices/:deviceId/registration', authLimiter, asyncRoute(async (req, res) => {
    service.assertEnabled('remote');
    const account = await service.authorizeDesktopBearer(bearerToken(req));
    const revoked = await service.revokeDevice(req.params.tenant, account.accountKey, req.params.deviceId);
    res.status(revoked ? 204 : 404).end();
  }));

  router.get('/remote-jobs', requireSession, asyncRoute(async (req, res) => {
    res.json({ jobs: await service.listJobs(req.params.tenant, req.webSession.accountKey, Number(req.query.limit) || 50) });
  }));
  router.post('/remote-jobs', jobLimiter, requireSession, requireCsrf, asyncRoute(async (req, res) => {
    const job = await service.createRemoteJob(req.params.tenant, req.webSession.accountKey, req.body || {});
    res.status(job.state === 'accepted' ? 202 : 200).json(job);
  }));
  router.post('/remote-jobs/:jobId/cancel', jobLimiter, requireSession, requireCsrf, asyncRoute(async (req, res) => {
    res.json(await service.cancelRemoteJob(req.params.tenant, req.webSession.accountKey, req.params.jobId));
  }));

  router.get('/remote-events', requireSession, (req, res) => {
    res.status(200);
    res.set({
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache, no-transform',
      Connection: 'keep-alive',
      'X-Accel-Buffering': 'no'
    });
    res.flushHeaders();
    res.write(`event: ready\ndata: ${JSON.stringify({ serverTime: new Date().toISOString() })}\n\n`);
    const unsubscribe = eventHub.subscribe(req.webSession.accountKey, (event) => {
      res.write(`id: ${event.id}\nevent: ${event.type}\ndata: ${JSON.stringify(event.payload)}\n\n`);
    }, req.get('last-event-id'));
    const heartbeat = setInterval(() => res.write(': heartbeat\n\n'), 20_000);
    req.on('close', () => {
      clearInterval(heartbeat);
      unsubscribe();
    });
  });

  return router;
}

module.exports = { createRemoteRouter };
