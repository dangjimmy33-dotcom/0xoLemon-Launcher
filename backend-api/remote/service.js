const crypto = require('crypto');
const { DiscordClient } = require('../activation/discord-client');
const { accountKey } = require('./config');
const { hashToken, openJson, randomToken, sealJson } = require('./security');

const DEVICE_ID = /^[a-zA-Z0-9_-]{12,96}$/;
const LIBRARY_ID = /^[a-zA-Z0-9_-]{1,96}$/;
const GAME_ID = /^[a-z0-9][a-z0-9._-]{0,127}$/i;
const VERSION_ID = /^[^\u0000-\u001f\u007f]{1,160}$/;
const REQUEST_ID = /^[a-zA-Z0-9_-]{16,128}$/;
const REMOTE_ACTIONS = new Set(['install', 'update', 'downgrade', 'repair', 'verify', 'launch']);
const TERMINAL_STATES = new Set(['completed', 'failed', 'canceled']);

function cleanText(value, max) {
  return typeof value === 'string'
    ? value.replace(/[\u0000-\u001f\u007f]/g, ' ').trim().slice(0, max)
    : '';
}

function publicProfile(profile) {
  return {
    id: profile.id,
    username: profile.username,
    displayName: profile.displayName,
    avatarUrl: profile.avatarUrl || ''
  };
}

function publicDevice(data, online) {
  return {
    id: data.deviceId,
    name: data.name,
    os: data.os,
    launcherVersion: data.launcherVersion,
    online,
    lastSeen: data.lastSeen || null,
    libraries: Array.isArray(data.libraries) ? data.libraries : [],
    installedGameIds: Array.isArray(data.installedGameIds) ? data.installedGameIds : []
  };
}

function publicJob(data) {
  return {
    id: data.id,
    requestId: data.requestId,
    action: data.action,
    gameId: data.gameId,
    versionId: data.versionId,
    deviceId: data.deviceId,
    libraryId: data.libraryId,
    state: data.state,
    progress: data.progress || null,
    errorCode: data.errorCode || null,
    errorMessage: data.errorMessage || null,
    createdAt: data.createdAt || null,
    updatedAt: data.updatedAt || null
  };
}

function sameRemoteRequest(job, input) {
  return (
    job.action === input.action &&
    job.gameId === input.gameId &&
    String(job.versionId || '') === String(input.versionId || '') &&
    String(job.libraryId || '') === String(input.libraryId || '') &&
    (!input.deviceId || job.deviceId === input.deviceId)
  );
}

function replayExistingJob(snapshot, input) {
  const job = snapshot.data();
  if (!sameRemoteRequest(job, input)) {
    throw Object.assign(new Error('REQUEST_ID_CONFLICT'), { status: 409 });
  }
  return publicJob(job);
}

function remoteAssetUrl(value) {
  if (typeof value !== 'string') return '';
  if (/^https:\/\//i.test(value)) return value;
  if (value.startsWith('remote:')) {
    const url = value.slice('remote:'.length);
    return /^https:\/\//i.test(url) ? url : '';
  }
  if (value.startsWith('remote64:')) {
    try {
      const url = Buffer.from(value.slice('remote64:'.length), 'base64url').toString('utf8');
      return /^https:\/\//i.test(url) ? url : '';
    } catch {
      return '';
    }
  }
  return '';
}

class RemoteService {
  constructor({ getTenantDb, config, gateway, eventHub, now = () => Date.now() }) {
    this.getTenantDb = getTenantDb;
    this.config = config;
    this.gateway = gateway;
    this.eventHub = eventHub;
    this.now = now;
    this.discord = new DiscordClient(config.discord, now);
  }

  assertEnabled(feature = 'web') {
    if (feature === 'remote' ? !this.config.remoteWebEnabled : !this.config.webAuthEnabled) {
      const error = new Error(feature === 'remote' ? 'REMOTE_WEB_DISABLED' : 'WEB_AUTH_DISABLED');
      error.status = 503;
      throw error;
    }
  }

  assertCanary(profile) {
    if (this.config.canaryDiscordIds.size && !this.config.canaryDiscordIds.has(profile.id)) {
      const error = new Error('REMOTE_CANARY_REQUIRED');
      error.status = 403;
      throw error;
    }
  }

  async exchangeDiscordCode({ code, verifier }) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.config.discord.timeoutMs);
    try {
      const body = new URLSearchParams({
        client_id: this.config.discord.clientId,
        client_secret: this.config.discord.clientSecret,
        grant_type: 'authorization_code',
        code,
        redirect_uri: this.config.discord.redirectUri,
        code_verifier: verifier
      });
      const response = await fetch(this.config.discord.tokenUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body,
        signal: controller.signal
      });
      if (!response.ok) {
        const error = new Error('DISCORD_CODE_EXCHANGE_FAILED');
        error.status = 401;
        throw error;
      }
      const grant = await response.json();
      if (!grant.access_token || !grant.refresh_token || !Number.isFinite(Number(grant.expires_in))) {
        const error = new Error('DISCORD_GRANT_INVALID');
        error.status = 502;
        throw error;
      }
      return {
        accessToken: grant.access_token,
        refreshToken: grant.refresh_token,
        expiresAt: this.now() + Number(grant.expires_in) * 1000
      };
    } finally {
      clearTimeout(timer);
    }
  }

  async refreshDiscordGrant(grant) {
    const body = new URLSearchParams({
      client_id: this.config.discord.clientId,
      client_secret: this.config.discord.clientSecret,
      grant_type: 'refresh_token',
      refresh_token: grant.refreshToken
    });
    const response = await fetch(this.config.discord.tokenUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body
    });
    if (!response.ok) {
      const error = new Error('WEB_SESSION_REVOKED');
      error.status = 401;
      throw error;
    }
    const value = await response.json();
    return {
      accessToken: value.access_token,
      refreshToken: value.refresh_token || grant.refreshToken,
      expiresAt: this.now() + Number(value.expires_in || 3600) * 1000
    };
  }

  async createWebSession(tenantId, grant) {
    const profile = await this.discord.authorizeProfile(grant.accessToken);
    this.assertCanary(profile);
    const id = randomToken(48);
    const csrf = randomToken(32);
    const createdAt = new Date(this.now()).toISOString();
    const expiresAt = new Date(this.now() + this.config.sessionTtlMs).toISOString();
    const key = accountKey(this.config, profile.id);
    await this.getTenantDb(tenantId).collection('webSessions').doc(hashToken(id)).set({
      accountKey: key,
      profile: publicProfile(profile),
      csrfHash: hashToken(csrf),
      encryptedDiscordGrant: sealJson(grant, this.config.sessionKey),
      lastDiscordVerificationAt: createdAt,
      createdAt,
      expiresAt,
      revokedAt: null
    });
    return { id, csrf, expiresAt, accountKey: key, profile: publicProfile(profile) };
  }

  async getSession(tenantId, sessionId, { refreshIdentity = true } = {}) {
    if (!sessionId || sessionId.length > 256) return null;
    const ref = this.getTenantDb(tenantId).collection('webSessions').doc(hashToken(sessionId));
    const snapshot = await ref.get();
    if (!snapshot.exists) return null;
    const data = snapshot.data();
    if (data.revokedAt || Date.parse(data.expiresAt || '') <= this.now()) return null;

    const verifiedAt = Date.parse(data.lastDiscordVerificationAt || '') || 0;
    if (refreshIdentity && this.now() - verifiedAt > 10 * 60 * 1000) {
      let grant = openJson(data.encryptedDiscordGrant, this.config.sessionKey);
      if (!grant) return null;
      if (Number(grant.expiresAt || 0) <= this.now() + 30_000) grant = await this.refreshDiscordGrant(grant);
      const profile = await this.discord.authorizeProfile(grant.accessToken);
      this.assertCanary(profile);
      const refreshed = {
        profile: publicProfile(profile),
        encryptedDiscordGrant: sealJson(grant, this.config.sessionKey),
        lastDiscordVerificationAt: new Date(this.now()).toISOString()
      };
      await ref.update(refreshed);
      Object.assign(data, refreshed);
    }
    return { ref, ...data };
  }

  async revokeSession(tenantId, sessionId) {
    if (!sessionId) return;
    const ref = this.getTenantDb(tenantId).collection('webSessions').doc(hashToken(sessionId));
    const snapshot = await ref.get();
    if (snapshot.exists) await ref.update({ revokedAt: new Date(this.now()).toISOString() });
  }

  async getLegalAcceptance(tenantId, key) {
    const snapshot = await this.getTenantDb(tenantId).collection('legalAcceptances').doc(key).get();
    const data = snapshot.exists ? snapshot.data() : null;
    const current = Boolean(
      data &&
      data.termsVersion === this.config.legal.termsVersion &&
      data.privacyVersion === this.config.legal.privacyVersion
    );
    return {
      accepted: current,
      termsVersion: this.config.legal.termsVersion,
      privacyVersion: this.config.legal.privacyVersion,
      acceptedAt: current ? data.acceptedAt : null,
      locale: current ? data.locale : null
    };
  }

  async acceptLegal(tenantId, key, body) {
    if (body.termsVersion !== this.config.legal.termsVersion || body.privacyVersion !== this.config.legal.privacyVersion) {
      const error = new Error('LEGAL_VERSION_MISMATCH');
      error.status = 409;
      throw error;
    }
    const value = {
      termsVersion: this.config.legal.termsVersion,
      privacyVersion: this.config.legal.privacyVersion,
      locale: body.locale === 'vi' ? 'vi' : 'en',
      acceptedAt: new Date(this.now()).toISOString()
    };
    await this.getTenantDb(tenantId).collection('legalAcceptances').doc(key).set(value);
    return { accepted: true, ...value };
  }

  async getWebCatalog(tenantId) {
    const db = this.getTenantDb(tenantId);
    const [catalogSnapshot, assetsSnapshot] = await Promise.all([
      db.collection('config').doc('gameCatalog').get(),
      db.collection('config').doc('assets_override').get()
    ]);
    if (!catalogSnapshot.exists) throw Object.assign(new Error('CATALOG_NOT_FOUND'), { status: 404 });
    const catalog = catalogSnapshot.data() || {};
    const overrides = assetsSnapshot.exists ? assetsSnapshot.data() || {} : {};
    const games = Array.isArray(catalog.games) ? catalog.games.slice(0, 5000).map((game) => {
      const id = String(game.id || '');
      return {
        id,
        title: cleanText(game.title, 160) || id,
        subtitle: cleanText(game.subtitle, 240),
        developer: cleanText(game.developer, 160),
        publisher: cleanText(game.publisher, 160),
        latestVersion: cleanText(game.latestVersion, 160),
        availableVersions: Array.isArray(game.availableVersions)
          ? game.availableVersions.slice(0, 100).map((version) => ({
              version: cleanText(version.version, 160),
              label: cleanText(version.label, 200),
              buildId: cleanText(version.buildId, 80),
              sizeBytes: Math.max(0, Math.floor(Number(version.sizeBytes) || 0)),
              latest: Boolean(version.latest)
            })).filter((version) => version.version)
          : [],
        gridAssetUrl: remoteAssetUrl(overrides[`${id}-grid`]) || remoteAssetUrl(game.gridAssetId),
        heroAssetUrl: remoteAssetUrl(overrides[`${id}-hero`]) || remoteAssetUrl(game.heroAssetId)
      };
    }).filter((game) => GAME_ID.test(game.id)) : [];
    return { defaultLocale: cleanText(catalog.defaultLocale, 20) || 'en-US', games };
  }

  async authorizeDesktopBearer(accessToken) {
    const profile = await this.discord.authorizeProfile(accessToken);
    this.assertCanary(profile);
    return { profile: publicProfile(profile), accountKey: accountKey(this.config, profile.id) };
  }

  sanitizeDeviceState(body) {
    const libraries = Array.isArray(body.libraries) ? body.libraries.slice(0, 32).map((entry) => ({
      id: LIBRARY_ID.test(String(entry.id || '')) ? String(entry.id) : '',
      label: cleanText(entry.label, 80),
      freeBytes: Math.max(0, Math.floor(Number(entry.freeBytes) || 0)),
      default: Boolean(entry.default)
    })).filter((entry) => entry.id) : [];
    return {
      name: cleanText(body.name, 80) || 'Windows PC',
      os: cleanText(body.os, 80) || 'Windows',
      launcherVersion: cleanText(body.launcherVersion, 32),
      libraries,
      installedGameIds: Array.isArray(body.installedGameIds)
        ? [...new Set(body.installedGameIds.map(String).filter((id) => GAME_ID.test(id)))].slice(0, 1000)
        : []
    };
  }

  async registerDevice(tenantId, account, body) {
    this.assertEnabled('remote');
    const requestedId = String(body.deviceId || '');
    const deviceId = DEVICE_ID.test(requestedId) ? requestedId : randomToken(24);
    const credential = randomToken(48);
    const state = this.sanitizeDeviceState(body);
    const now = new Date(this.now()).toISOString();
    const ref = this.getTenantDb(tenantId).collection('launcherDevices').doc(`${account.accountKey}_${deviceId}`);
    await ref.set({
      accountKey: account.accountKey,
      deviceId,
      credentialHash: hashToken(credential),
      ...state,
      registeredAt: now,
      lastSeen: now,
      revokedAt: null
    }, { merge: true });
    return { deviceId, credential, ...state };
  }

  async authenticateDevice(tenantId, deviceId, credential) {
    if (!DEVICE_ID.test(String(deviceId || '')) || typeof credential !== 'string') return null;
    const db = this.getTenantDb(tenantId);
    const query = await db.collection('launcherDevices').where('deviceId', '==', deviceId).limit(2).get();
    if (query.empty || query.size !== 1) return null;
    const document = query.docs[0];
    const data = document.data();
    const supplied = Buffer.from(hashToken(credential), 'hex');
    const expected = Buffer.from(String(data.credentialHash || ''), 'hex');
    if (supplied.length !== expected.length || !crypto.timingSafeEqual(supplied, expected) || data.revokedAt) return null;
    return { ref: document.ref, ...data };
  }

  async updateDeviceState(tenantId, device, state) {
    const clean = this.sanitizeDeviceState(state);
    const lastSeen = new Date(this.now()).toISOString();
    await device.ref.set({ ...clean, lastSeen }, { merge: true });
    Object.assign(device, clean, { lastSeen });
    this.eventHub.publish(device.accountKey, 'device.updated', publicDevice(device, true));
  }

  async listDevices(tenantId, key) {
    const snapshot = await this.getTenantDb(tenantId).collection('launcherDevices').where('accountKey', '==', key).get();
    return snapshot.docs
      .map((doc) => doc.data())
      .filter((device) => !device.revokedAt)
      .map((device) => publicDevice(device, this.gateway.isOnline(key, device.deviceId)))
      .sort((a, b) => Number(b.online) - Number(a.online) || String(b.lastSeen).localeCompare(String(a.lastSeen)));
  }

  async revokeDevice(tenantId, key, deviceId) {
    const ref = this.getTenantDb(tenantId).collection('launcherDevices').doc(`${key}_${deviceId}`);
    const snapshot = await ref.get();
    if (!snapshot.exists || snapshot.data().accountKey !== key) return false;
    await ref.update({ revokedAt: new Date(this.now()).toISOString() });
    const connection = this.gateway.connections.get(this.gateway.key(key, deviceId));
    if (connection) connection.socket.close(4003, 'Device access revoked');
    return true;
  }

  validateRemoteJob(body) {
    const action = String(body.action || '');
    const gameId = String(body.gameId || '');
    const versionId = String(body.versionId || '');
    const deviceId = String(body.deviceId || '');
    const libraryId = String(body.libraryId || '');
    const requestId = String(body.requestId || '');
    if (!REMOTE_ACTIONS.has(action)) throw Object.assign(new Error('REMOTE_ACTION_INVALID'), { status: 400 });
    if (!GAME_ID.test(gameId)) throw Object.assign(new Error('GAME_ID_INVALID'), { status: 400 });
    if (action !== 'launch' && !VERSION_ID.test(versionId)) throw Object.assign(new Error('VERSION_ID_INVALID'), { status: 400 });
    if (deviceId && !DEVICE_ID.test(deviceId)) throw Object.assign(new Error('DEVICE_ID_INVALID'), { status: 400 });
    if (action !== 'launch' && !LIBRARY_ID.test(libraryId)) throw Object.assign(new Error('LIBRARY_ID_INVALID'), { status: 400 });
    if (!REQUEST_ID.test(requestId)) throw Object.assign(new Error('REQUEST_ID_INVALID'), { status: 400 });
    return { action, gameId, versionId, deviceId, libraryId, requestId };
  }

  async createRemoteJob(tenantId, key, body) {
    this.assertEnabled('remote');
    const input = this.validateRemoteJob(body);
    const legal = await this.getLegalAcceptance(tenantId, key);
    if (!legal.accepted) throw Object.assign(new Error('LEGAL_ACCEPTANCE_REQUIRED'), { status: 428 });
    const id = hashToken(`${key}:${input.requestId}`).slice(0, 40);
    const ref = this.getTenantDb(tenantId).collection('remoteJobs').doc(id);
    const existing = await ref.get();
    if (existing.exists) return replayExistingJob(existing, input);

    if (!input.deviceId) {
      const onlineDeviceIds = this.gateway.onlineDeviceIds(key);
      if (!onlineDeviceIds.length) throw Object.assign(new Error('DEVICE_UNAVAILABLE'), { status: 409 });
      if (onlineDeviceIds.length > 1) throw Object.assign(new Error('DEVICE_SELECTION_REQUIRED'), { status: 409 });
      input.deviceId = onlineDeviceIds[0];
    }
    if (!this.gateway.isOnline(key, input.deviceId)) throw Object.assign(new Error('DEVICE_UNAVAILABLE'), { status: 409 });
    const devices = await this.listDevices(tenantId, key);
    const target = devices.find((device) => device.id === input.deviceId && device.online);
    if (!target) throw Object.assign(new Error('DEVICE_UNAVAILABLE'), { status: 409 });
    if (input.action !== 'launch' && !target.libraries.some((library) => library.id === input.libraryId)) {
      throw Object.assign(new Error('LIBRARY_NOT_REGISTERED'), { status: 400 });
    }
    if (input.action === 'launch' && !target.installedGameIds.includes(input.gameId)) {
      throw Object.assign(new Error('GAME_NOT_INSTALLED'), { status: 409 });
    }

    const now = new Date(this.now()).toISOString();
    const job = { id, accountKey: key, ...input, state: 'dispatching', progress: null, createdAt: now, updatedAt: now };
    try {
      await ref.create(job);
    } catch (error) {
      // Firestore create is the idempotency boundary. A concurrent request with
      // the same requestId must observe the first job instead of dispatching a
      // second command to the launcher.
      const concurrent = await ref.get();
      if (concurrent.exists) return replayExistingJob(concurrent, input);
      throw error;
    }
    this.eventHub.publish(key, 'job.updated', publicJob(job));
    const ack = await this.gateway.dispatch(key, input.deviceId, publicJob(job));
    if (!ack.accepted) {
      const failed = { state: 'failed', errorCode: ack.detail || 'DEVICE_UNAVAILABLE', errorMessage: 'The launcher did not accept this request.', updatedAt: new Date(this.now()).toISOString() };
      await ref.update(failed);
      Object.assign(job, failed);
    } else {
      const accepted = { state: 'accepted', updatedAt: new Date(this.now()).toISOString() };
      await ref.update(accepted);
      Object.assign(job, accepted);
    }
    this.eventHub.publish(key, 'job.updated', publicJob(job));
    return publicJob(job);
  }

  async listJobs(tenantId, key, limit = 50) {
    const snapshot = await this.getTenantDb(tenantId).collection('remoteJobs')
      .where('accountKey', '==', key).limit(Math.min(Math.max(limit, 1), 100)).get();
    return snapshot.docs.map((doc) => publicJob(doc.data()))
      .sort((a, b) => String(b.createdAt).localeCompare(String(a.createdAt)));
  }

  async listRecoverableJobsForDevice(tenantId, key, deviceId) {
    const snapshot = await this.getTenantDb(tenantId).collection('remoteJobs')
      .where('accountKey', '==', key).limit(100).get();
    return snapshot.docs
      .map((doc) => doc.data())
      .filter((job) => job.deviceId === deviceId && !TERMINAL_STATES.has(job.state))
      .sort((left, right) => String(left.createdAt).localeCompare(String(right.createdAt)))
      .map(publicJob);
  }

  async cancelRemoteJob(tenantId, key, jobId) {
    if (!/^[a-f0-9]{40}$/.test(String(jobId || ''))) {
      throw Object.assign(new Error('REMOTE_JOB_ID_INVALID'), { status: 400 });
    }
    const ref = this.getTenantDb(tenantId).collection('remoteJobs').doc(jobId);
    const snapshot = await ref.get();
    if (!snapshot.exists || snapshot.data().accountKey !== key) {
      throw Object.assign(new Error('REMOTE_JOB_NOT_FOUND'), { status: 404 });
    }
    const job = snapshot.data();
    if (TERMINAL_STATES.has(job.state)) return publicJob(job);
    const updatedAt = new Date(this.now()).toISOString();
    const update = { state: 'canceled', updatedAt };
    await ref.update(update);
    Object.assign(job, update);
    this.gateway.send(key, job.deviceId, { type: 'remoteJob.cancel', jobId: job.id });
    this.eventHub.publish(key, 'job.updated', publicJob(job));
    return publicJob(job);
  }

  async updateJobFromDevice(tenantId, device, message) {
    const id = String(message.jobId || '');
    if (!/^[a-f0-9]{40}$/.test(id)) return false;
    const ref = this.getTenantDb(tenantId).collection('remoteJobs').doc(id);
    const snapshot = await ref.get();
    if (!snapshot.exists) return false;
    const job = snapshot.data();
    if (job.accountKey !== device.accountKey || job.deviceId !== device.deviceId || TERMINAL_STATES.has(job.state)) return false;
    const now = new Date(this.now()).toISOString();
    let update;
    if (message.type === 'remoteJob.progress') {
      const previous = Number(job.progress?.overallProgress || 0);
      const next = Math.max(previous, Math.min(1, Number(message.progress?.overallProgress || 0)));
      const bytesDone = Math.max(
        Number(job.progress?.bytesDone || 0),
        Math.floor(Number(message.progress?.bytesDone) || 0)
      );
      update = {
        state: 'running',
        progress: {
          overallProgress: next,
          phase: cleanText(message.progress?.phase, 80),
          bytesDone,
          bytesTotal: Math.max(
            bytesDone,
            Number(job.progress?.bytesTotal || 0),
            Math.floor(Number(message.progress?.bytesTotal) || 0)
          ),
          speedBytesPerSecond: Math.max(0, Math.floor(Number(message.progress?.speedBytesPerSecond) || 0))
        },
        updatedAt: now
      };
    } else if (message.type === 'remoteJob.complete') {
      update = { state: 'completed', progress: { ...(job.progress || {}), overallProgress: 1 }, updatedAt: now };
    } else if (message.type === 'remoteJob.fail') {
      update = { state: 'failed', errorCode: cleanText(message.errorCode, 80) || 'REMOTE_JOB_FAILED', errorMessage: cleanText(message.errorMessage, 300), updatedAt: now };
    } else if (message.type === 'remoteJob.canceled') {
      update = { state: 'canceled', updatedAt: now };
    } else {
      return false;
    }
    await ref.update(update);
    Object.assign(job, update);
    this.eventHub.publish(job.accountKey, 'job.updated', publicJob(job));
    return true;
  }
}

module.exports = { RemoteService, publicJob };
