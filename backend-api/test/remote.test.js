const assert = require('node:assert/strict');
const test = require('node:test');

const { RemoteEventHub } = require('../remote/events');
const { DeviceGateway } = require('../remote/gateway');
const { RemoteService } = require('../remote/service');
const {
  cookie,
  openJson,
  parseCookies,
  requireSameOrigin,
  safeReturnPath,
  sealJson
} = require('../remote/security');

function clone(value) {
  return value == null ? value : structuredClone(value);
}

class MemorySnapshot {
  constructor(ref, value) {
    this.ref = ref;
    this.exists = value !== undefined;
    this.value = clone(value);
    this.id = ref.id;
  }

  data() {
    return clone(this.value);
  }
}

class MemoryDocumentRef {
  constructor(db, collectionName, id) {
    this.db = db;
    this.collectionName = collectionName;
    this.id = id;
  }

  bucket() {
    if (!this.db.collections.has(this.collectionName)) {
      this.db.collections.set(this.collectionName, new Map());
    }
    return this.db.collections.get(this.collectionName);
  }

  async get() {
    return new MemorySnapshot(this, this.bucket().get(this.id));
  }

  async set(value, options = {}) {
    const next = options.merge
      ? { ...(this.bucket().get(this.id) || {}), ...clone(value) }
      : clone(value);
    this.bucket().set(this.id, next);
  }

  async create(value) {
    if (this.bucket().has(this.id)) {
      const error = new Error('ALREADY_EXISTS');
      error.code = 6;
      throw error;
    }
    this.bucket().set(this.id, clone(value));
  }

  async update(value) {
    if (!this.bucket().has(this.id)) throw new Error('NOT_FOUND');
    this.bucket().set(this.id, { ...this.bucket().get(this.id), ...clone(value) });
  }
}

class MemoryQuery {
  constructor(db, collectionName, filters = [], maximum = Number.MAX_SAFE_INTEGER) {
    this.db = db;
    this.collectionName = collectionName;
    this.filters = filters;
    this.maximum = maximum;
  }

  where(field, operator, value) {
    assert.equal(operator, '==');
    return new MemoryQuery(this.db, this.collectionName, [...this.filters, [field, value]], this.maximum);
  }

  limit(maximum) {
    return new MemoryQuery(this.db, this.collectionName, this.filters, maximum);
  }

  async get() {
    const bucket = this.db.collections.get(this.collectionName) || new Map();
    const docs = [...bucket.entries()]
      .filter(([, value]) => this.filters.every(([field, expected]) => value[field] === expected))
      .slice(0, this.maximum)
      .map(([id, value]) => new MemorySnapshot(new MemoryDocumentRef(this.db, this.collectionName, id), value));
    return { docs, empty: docs.length === 0, size: docs.length };
  }
}

class MemoryCollection extends MemoryQuery {
  doc(id) {
    return new MemoryDocumentRef(this.db, this.collectionName, id);
  }
}

class MemoryDb {
  constructor() {
    this.collections = new Map();
  }

  collection(name) {
    return new MemoryCollection(this, name);
  }
}

class FakeSocket {
  constructor(onSend = null) {
    this.readyState = 1;
    this.sent = [];
    this.onSend = onSend;
  }

  send(payload) {
    const message = JSON.parse(payload);
    this.sent.push(message);
    this.onSend?.(message);
  }

  close() {
    this.readyState = 3;
  }
}

function serviceFixture() {
  let now = Date.UTC(2026, 7, 26, 12, 0, 0);
  const db = new MemoryDb();
  const eventHub = new RemoteEventHub();
  const gateway = new DeviceGateway(eventHub, () => now);
  const config = {
    webAuthEnabled: true,
    remoteWebEnabled: true,
    canaryDiscordIds: new Set(),
    sessionTtlMs: 30 * 24 * 60 * 60 * 1000,
    sessionKey: Buffer.alloc(32, 9),
    accountHmacKey: 'remote-test-hmac-key-that-is-long-enough',
    legal: { termsVersion: 'terms-v1', privacyVersion: 'privacy-v1' },
    discord: {
      apiBase: 'https://discord.invalid',
      clientId: 'client',
      clientSecret: 'secret',
      guildId: 'guild',
      allowedRoleIds: ['role'],
      minimumAccountAgeMs: 0,
      timeoutMs: 1000,
      tokenUrl: 'https://discord.invalid/token',
      redirectUri: 'https://example.test/callback'
    }
  };
  const service = new RemoteService({ getTenantDb: () => db, config, gateway, eventHub, now: () => now });
  return { db, eventHub, gateway, service, advance: (milliseconds) => { now += milliseconds; } };
}

async function seedLegal(db, accountKey) {
  await db.collection('legalAcceptances').doc(accountKey).set({
    termsVersion: 'terms-v1',
    privacyVersion: 'privacy-v1',
    acceptedAt: '2026-08-26T12:00:00.000Z',
    locale: 'vi'
  });
}

async function seedDevice(db, accountKey, deviceId, overrides = {}) {
  await db.collection('launcherDevices').doc(`${accountKey}_${deviceId}`).set({
    accountKey,
    deviceId,
    credentialHash: 'unused-in-service-tests',
    name: deviceId,
    os: 'Windows 11',
    launcherVersion: '2.0.50',
    libraries: [{ id: 'library-main', label: 'Game library', freeBytes: 500_000_000_000, default: true }],
    installedGameIds: [],
    lastSeen: '2026-08-26T12:00:00.000Z',
    revokedAt: null,
    ...overrides
  });
}

function jobRequest(overrides = {}) {
  return {
    action: 'install',
    gameId: 'avatar-frontiers-of-pandora',
    versionId: '1.0.0',
    libraryId: 'library-main',
    requestId: 'request-remote-00000001',
    ...overrides
  };
}

test('session helpers seal state, enforce cookie flags, and reject unsafe return paths', () => {
  const key = Buffer.alloc(32, 7);
  const sealed = sealJson({ verifier: 'secret', returnTo: '/app' }, key);
  assert.deepEqual(openJson(sealed, key), { verifier: 'secret', returnTo: '/app' });
  assert.equal(openJson(`${sealed}tampered`, key), null);
  assert.equal(safeReturnPath('//attacker.example/path'), '/app');
  assert.equal(safeReturnPath('https://attacker.example'), '/app');
  assert.equal(safeReturnPath('/app'), '/app');
  const header = cookie('__Host-session', 'opaque', { httpOnly: true });
  assert.match(header, /HttpOnly/);
  assert.match(header, /Secure/);
  assert.match(header, /SameSite=Lax/);
  assert.equal(parseCookies('a=1; csrf=encoded%20value').csrf, 'encoded value');
  const request = { get: (name) => name === 'origin' ? 'https://allowed.test' : '' };
  assert.equal(requireSameOrigin(request, { allowedOrigins: new Set(['https://allowed.test']) }), true);
  assert.equal(requireSameOrigin(request, { allowedOrigins: new Set(['https://other.test']) }), false);
});

test('device ACK is bound to the account, device, and one-time dispatch nonce', async () => {
  const eventHub = new RemoteEventHub();
  const gateway = new DeviceGateway(eventHub);
  const socket = new FakeSocket();
  gateway.attach('account-a', 'device-alpha-0001', socket);
  const pending = gateway.dispatch('account-a', 'device-alpha-0001', { id: 'a'.repeat(40) }, 100);
  const dispatch = socket.sent.at(-1);
  assert.equal(gateway.acknowledge('account-b', 'device-alpha-0001', dispatch.job.id, dispatch.dispatchNonce, true), false);
  assert.equal(gateway.acknowledge('account-a', 'device-beta-00002', dispatch.job.id, dispatch.dispatchNonce, true), false);
  assert.equal(gateway.acknowledge('account-a', 'device-alpha-0001', dispatch.job.id, 'wrong-nonce', true), false);
  assert.equal(gateway.acknowledge('account-a', 'device-alpha-0001', dispatch.job.id, dispatch.dispatchNonce, true), true);
  assert.deepEqual(await pending, { accepted: true, detail: '' });
});

test('SSE event history replays only missed events for the same account', () => {
  const hub = new RemoteEventHub(4);
  const first = hub.publish('account-a', 'device.online', { deviceId: 'one' });
  hub.publish('account-b', 'device.online', { deviceId: 'other' });
  const third = hub.publish('account-a', 'job.updated', { id: 'job' });
  const replayed = [];
  const unsubscribe = hub.subscribe('account-a', (event) => replayed.push(event), first.id);
  unsubscribe();
  assert.deepEqual(replayed.map((event) => event.id), [third.id]);
});

test('one online device is selected automatically and a retry remains idempotent after disconnect', async () => {
  const fixture = serviceFixture();
  const accountKey = 'account-one';
  const deviceId = 'device-alpha-0001';
  await seedLegal(fixture.db, accountKey);
  await seedDevice(fixture.db, accountKey, deviceId);
  const socket = new FakeSocket((message) => {
    if (message.type === 'remoteJob.dispatch') {
      setImmediate(() => fixture.gateway.acknowledge(
        accountKey,
        deviceId,
        message.job.id,
        message.dispatchNonce,
        true
      ));
    }
  });
  const connection = fixture.gateway.attach(accountKey, deviceId, socket);
  const request = jobRequest();
  const created = await fixture.service.createRemoteJob('tenant', accountKey, request);
  assert.equal(created.state, 'accepted');
  assert.equal(created.deviceId, deviceId);
  assert.equal(socket.sent.filter((message) => message.type === 'remoteJob.dispatch').length, 1);

  fixture.gateway.detach(connection);
  const replay = await fixture.service.createRemoteJob('tenant', accountKey, request);
  assert.deepEqual(replay, created);
  await assert.rejects(
    fixture.service.createRemoteJob('tenant', accountKey, { ...request, gameId: 'different-game' }),
    (error) => error.message === 'REQUEST_ID_CONFLICT' && error.status === 409
  );
});

test('multiple online devices require an explicit destination and offline devices cannot queue work', async () => {
  const fixture = serviceFixture();
  const accountKey = 'account-many';
  await seedLegal(fixture.db, accountKey);
  for (const deviceId of ['device-alpha-0001', 'device-beta-00002']) {
    await seedDevice(fixture.db, accountKey, deviceId);
    fixture.gateway.attach(accountKey, deviceId, new FakeSocket());
  }
  await assert.rejects(
    fixture.service.createRemoteJob('tenant', accountKey, jobRequest({ requestId: 'request-remote-00000002' })),
    (error) => error.message === 'DEVICE_SELECTION_REQUIRED' && error.status === 409
  );

  const offline = serviceFixture();
  await seedLegal(offline.db, accountKey);
  await seedDevice(offline.db, accountKey, 'device-alpha-0001');
  await assert.rejects(
    offline.service.createRemoteJob('tenant', accountKey, jobRequest({ requestId: 'request-remote-00000003' })),
    (error) => error.message === 'DEVICE_UNAVAILABLE' && error.status === 409
  );
});

test('concurrent retries dispatch once and device progress cannot move backwards', async () => {
  const fixture = serviceFixture();
  const accountKey = 'account-race';
  const deviceId = 'device-alpha-0001';
  await seedLegal(fixture.db, accountKey);
  await seedDevice(fixture.db, accountKey, deviceId);
  const socket = new FakeSocket((message) => {
    if (message.type === 'remoteJob.dispatch') {
      setImmediate(() => fixture.gateway.acknowledge(
        accountKey,
        deviceId,
        message.job.id,
        message.dispatchNonce,
        true
      ));
    }
  });
  fixture.gateway.attach(accountKey, deviceId, socket);
  const request = jobRequest({ requestId: 'request-remote-00000004' });
  const [left, right] = await Promise.all([
    fixture.service.createRemoteJob('tenant', accountKey, request),
    fixture.service.createRemoteJob('tenant', accountKey, request)
  ]);
  assert.equal(left.id, right.id);
  assert.equal(socket.sent.filter((message) => message.type === 'remoteJob.dispatch').length, 1);

  const device = { accountKey, deviceId };
  assert.equal(await fixture.service.updateJobFromDevice('tenant', device, {
    type: 'remoteJob.progress',
    jobId: left.id,
    progress: { overallProgress: 0.75, phase: 'download', bytesDone: 750, bytesTotal: 1000, speedBytesPerSecond: 100 }
  }), true);
  assert.equal(await fixture.service.updateJobFromDevice('tenant', device, {
    type: 'remoteJob.progress',
    jobId: left.id,
    progress: { overallProgress: 0.2, phase: 'retry', bytesDone: 200, bytesTotal: 500, speedBytesPerSecond: 50 }
  }), true);
  const snapshot = await fixture.db.collection('remoteJobs').doc(left.id).get();
  assert.equal(snapshot.data().progress.overallProgress, 0.75);
  assert.equal(snapshot.data().progress.bytesDone, 750);
  assert.equal(snapshot.data().progress.bytesTotal, 1000);
});
