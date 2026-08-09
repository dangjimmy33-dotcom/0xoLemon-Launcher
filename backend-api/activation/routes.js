const EventEmitter = require('events');
const express = require('express');
const rateLimit = require('express-rate-limit');

const { loadActivationConfig } = require('./config');
const { DiscordClient, bearerToken } = require('./discord-client');
const { EaClient, parseTicket } = require('./ea-client');
const { ActivationError, errorResponse } = require('./errors');
const { ActivationQuotaService, FirestoreActivationStore } = require('./quota-service');
const { SecretCrypto } = require('./secret-crypto');

const REQUEST_ID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const VERSION_PATTERN = /^\d+\.\d+\.\d+(?:[-+][A-Za-z0-9.-]+)?$/;

function semverParts(version) {
  return String(version || '').split(/[+-]/, 1)[0].split('.').map((part) => Number.parseInt(part, 10));
}

function versionAtLeast(version, minimum) {
  if (!minimum) return true;
  const left = semverParts(version);
  const right = semverParts(minimum);
  for (let index = 0; index < 3; index += 1) {
    if ((left[index] || 0) > (right[index] || 0)) return true;
    if ((left[index] || 0) < (right[index] || 0)) return false;
  }
  return true;
}

class StatusBroker {
  constructor() {
    this.events = new EventEmitter();
    this.events.setMaxListeners(0);
  }

  key(tenantId, gameId) { return `${tenantId}:${gameId}`; }
  publish(tenantId, gameId, event) { this.events.emit(this.key(tenantId, gameId), event); }
  subscribe(tenantId, gameId, listener) {
    const key = this.key(tenantId, gameId);
    this.events.on(key, listener);
    return () => this.events.off(key, listener);
  }
}

function publicPackage(config) {
  return {
    version: config.package.version || null,
    url: config.package.url || null,
    sizeBytes: config.package.sizeBytes || 0,
    sha256: config.package.sha256 || null,
    files: config.package.files
  };
}

function createOfflineActivationRouter({ getTenantDb }) {
  const router = express.Router({ mergeParams: true });
  const config = loadActivationConfig();
  const broker = new StatusBroker();
  const runtimes = new Map();

  const activationIpLimiter = rateLimit({
    windowMs: 15 * 60 * 1000,
    max: 12,
    standardHeaders: true,
    legacyHeaders: false,
    message: { code: 'IP_RATE_LIMITED', message: 'Too many activation attempts from this network.' }
  });

  function runtimeFor(tenantId, gameId) {
    if (tenantId !== config.tenantId || gameId !== config.gameId) {
      throw new ActivationError('UNSUPPORTED_GAME', 'Offline activation is only available for EA SPORTS FC 26.', { httpStatus: 404 });
    }
    const key = `${tenantId}:${gameId}`;
    if (runtimes.has(key)) return runtimes.get(key);
    const db = getTenantDb(tenantId);
    const quota = new ActivationQuotaService(new FirestoreActivationStore(db), config);
    let secretCrypto = null;
    let eaClient = null;
    let cryptoError = null;
    try {
      secretCrypto = new SecretCrypto(config.encryptionKey);
      eaClient = new EaClient(config.ea, db, secretCrypto);
    } catch (error) {
      cryptoError = error.message;
    }
    const runtime = {
      quota,
      secretCrypto,
      eaClient,
      discordClient: new DiscordClient(config.discord),
      cryptoError
    };
    runtimes.set(key, runtime);
    return runtime;
  }

  function readiness(runtime) {
    const missing = [...config.readiness.missingConfiguration];
    if (runtime.cryptoError && !missing.includes('ACTIVATION_ENCRYPTION_KEY')) {
      missing.push('ACTIVATION_ENCRYPTION_KEY_INVALID');
    }
    return { ready: missing.length === 0 && Boolean(runtime.eaClient), missingConfiguration: missing };
  }

  async function statusFor(req) {
    const runtime = runtimeFor(req.params.tenant, req.params.gameId);
    return {
      ...(await runtime.quota.status()),
      readiness: readiness(runtime),
      package: publicPackage(config)
    };
  }

  async function authenticate(req, runtime) {
    const user = await runtime.discordClient.authorize(bearerToken(req));
    if (!runtime.secretCrypto) {
      throw new ActivationError('SERVICE_UNAVAILABLE', 'Activation encryption is not configured.', { httpStatus: 503 });
    }
    return { accountKey: runtime.secretCrypto.accountKey(user.id) };
  }

  function sendError(res, error) {
    const status = error instanceof ActivationError ? error.httpStatus : 500;
    if (!(error instanceof ActivationError)) console.error('[activation] internal failure:', error.message);
    res.status(status).json(errorResponse(error));
  }

  async function publishStatus(req, event) {
    try {
      broker.publish(req.params.tenant, req.params.gameId, { event, state: await statusFor(req) });
    } catch (error) {
      console.error('[activation] status publish failed:', error.message);
    }
  }

  async function optionalStatus(req) {
    try {
      return await statusFor(req);
    } catch (error) {
      console.error('[activation] response status snapshot failed:', error.message);
      return null;
    }
  }

  router.get('/:gameId/status', async (req, res) => {
    try {
      res.set('cache-control', 'no-store');
      res.json(await statusFor(req));
    } catch (error) { sendError(res, error); }
  });

  router.get('/:gameId/events', async (req, res) => {
    try {
      runtimeFor(req.params.tenant, req.params.gameId);
      res.status(200);
      res.set({
        'content-type': 'text/event-stream',
        'cache-control': 'no-cache, no-transform',
        connection: 'keep-alive',
        'x-accel-buffering': 'no'
      });
      res.flushHeaders();
      const writeEvent = (payload) => res.write(`event: quota\ndata: ${JSON.stringify(payload)}\n\n`);
      writeEvent({ event: 'connected', state: await statusFor(req) });
      const unsubscribe = broker.subscribe(req.params.tenant, req.params.gameId, writeEvent);
      const heartbeat = setInterval(() => res.write(`: heartbeat ${Date.now()}\n\n`), 20000);
      const refresh = setInterval(async () => {
        try { writeEvent({ event: 'refresh', state: await statusFor(req) }); } catch { /* next interval retries */ }
      }, 60000);
      req.on('close', () => {
        clearInterval(heartbeat);
        clearInterval(refresh);
        unsubscribe();
      });
    } catch (error) { sendError(res, error); }
  });

  router.get('/:gameId/me', async (req, res) => {
    try {
      const runtime = runtimeFor(req.params.tenant, req.params.gameId);
      const { accountKey } = await authenticate(req, runtime);
      res.set('cache-control', 'no-store');
      res.json({
        ...(await runtime.quota.eligibility(accountKey)),
        packageAccess: {
          archivePassword: config.package.archivePassword
        }
      });
    } catch (error) { sendError(res, error); }
  });

  router.post('/:gameId/activate', activationIpLimiter, async (req, res) => {
    let runtime;
    let accountKey;
    let requestId;
    let reservationEstablished = false;
    let eaStarted = false;
    try {
      runtime = runtimeFor(req.params.tenant, req.params.gameId);
      const serviceReadiness = readiness(runtime);
      if (!serviceReadiness.ready) {
        throw new ActivationError('SERVICE_UNAVAILABLE', 'Offline activation is not fully configured.', { httpStatus: 503 });
      }
      ({ accountKey } = await authenticate(req, runtime));
      requestId = typeof req.body.requestId === 'string' ? req.body.requestId.trim() : '';
      const ticket = typeof req.body.ticket === 'string' ? req.body.ticket.trim() : '';
      const launcherVersion = typeof req.body.launcherVersion === 'string' ? req.body.launcherVersion.trim() : '';
      if (!REQUEST_ID_PATTERN.test(requestId)) {
        throw new ActivationError('INVALID_REQUEST', 'requestId must be a UUID.', { httpStatus: 400 });
      }
      if (!VERSION_PATTERN.test(launcherVersion) || !versionAtLeast(launcherVersion, config.minimumLauncherVersion)) {
        throw new ActivationError('LAUNCHER_UPDATE_REQUIRED', 'Update the launcher before using offline activation.', { httpStatus: 426 });
      }
      parseTicket(ticket);
      const ticketHash = runtime.secretCrypto.ticketHash(ticket);
      const reservation = await runtime.quota.reserve({ requestId, accountKey, ticketHash, launcherVersion });
      if (reservation.kind === 'replay') {
        const token = runtime.secretCrypto.decrypt(reservation.encryptedToken, `activation-token:${requestId}`);
        return res.json({ requestId, token, replayed: true, state: await optionalStatus(req) });
      }
      reservationEstablished = true;
      await publishStatus(req, 'reserved');

      const eaAccessToken = await runtime.eaClient.accessTokenForActivation();
      await runtime.quota.markStarted(requestId, accountKey);
      eaStarted = true;
      await publishStatus(req, 'started');
      const token = await runtime.eaClient.activate(ticket, eaAccessToken);
      const encryptedToken = runtime.secretCrypto.encrypt(token, `activation-token:${requestId}`);

      let completionError = null;
      for (let attempt = 0; attempt < 3; attempt += 1) {
        try {
          await runtime.quota.completeSuccess(requestId, accountKey, encryptedToken);
          completionError = null;
          break;
        } catch (error) {
          completionError = error;
          if (attempt < 2) await new Promise((resolve) => setTimeout(resolve, 200 * (attempt + 1)));
        }
      }
      if (completionError) throw completionError;
      await publishStatus(req, 'success');
      return res.json({ requestId, token, replayed: false, state: await optionalStatus(req) });
    } catch (error) {
      if (reservationEstablished && runtime && accountKey && requestId) {
        const uncertain = eaStarted && (error.uncertain || !['EA_REJECTED', 'INVALID_TICKET'].includes(error.code));
        try {
          await runtime.quota.completeFailure(requestId, accountKey, uncertain, error.code || 'INTERNAL_ERROR');
          await publishStatus(req, uncertain ? 'uncertain' : 'failed');
        } catch (cleanupError) {
          console.error('[activation] reservation finalization failed:', cleanupError.message);
        }
        if (uncertain && !(error instanceof ActivationError && error.uncertain)) {
          error = new ActivationError('OUTCOME_UNCERTAIN', 'EA may have consumed this activation. The slot remains held for ten hours.', {
            httpStatus: 503,
            uncertain: true
          });
        }
      }
      return sendError(res, error);
    }
  });

  router.post('/:gameId/cancel', activationIpLimiter, async (req, res) => {
    try {
      const runtime = runtimeFor(req.params.tenant, req.params.gameId);
      const { accountKey } = await authenticate(req, runtime);
      const requestId = typeof req.body.requestId === 'string' ? req.body.requestId.trim() : '';
      if (!REQUEST_ID_PATTERN.test(requestId)) {
        throw new ActivationError('INVALID_REQUEST', 'requestId must be a UUID.', { httpStatus: 400 });
      }
      await runtime.quota.cancel(requestId, accountKey);
      await publishStatus(req, 'canceled');
      res.json({ canceled: true });
    } catch (error) { sendError(res, error); }
  });

  return router;
}

module.exports = { createOfflineActivationRouter, versionAtLeast };
