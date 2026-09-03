const assert = require('node:assert/strict');
const crypto = require('crypto');
const test = require('node:test');
const sharp = require('sharp');

const { SocialEventHub } = require('../social/event-hub');
const { HfCoverPublisher, validateCoverBuffer } = require('../social/hf-cover-publisher');
const { createAccountLimiter } = require('../social/routes');
const { SocialService } = require('../social/service');

function clone(value) {
  return value == null ? value : structuredClone(value);
}

class MemoryDocumentSnapshot {
  constructor(ref, value) {
    this.ref = ref;
    this.id = ref.id;
    this.exists = value !== undefined;
    this.value = clone(value);
  }
  data() { return clone(this.value); }
}

class MemoryDocument {
  constructor(db, collectionName, id) {
    this.db = db;
    this.collectionName = collectionName;
    this.id = id;
  }
  async get() { return new MemoryDocumentSnapshot(this, this.db.read(this)); }
  async set(value, options) { this.db.write(this, value, options); }
  async delete() { this.db.values.delete(this.db.key(this)); }
}

class MemoryQuery {
  constructor(db, collectionName, filters = [], maximum = Infinity, afterId = '') {
    this.db = db;
    this.collectionName = collectionName;
    this.filters = filters;
    this.maximum = maximum;
    this.afterId = afterId;
  }
  where(field, operation, value) {
    return new MemoryQuery(this.db, this.collectionName, [...this.filters, { field, operation, value }], this.maximum, this.afterId);
  }
  limit(maximum) { return new MemoryQuery(this.db, this.collectionName, this.filters, maximum, this.afterId); }
  startAfter(snapshot) { return new MemoryQuery(this.db, this.collectionName, this.filters, this.maximum, snapshot.id); }
  async get() {
    const prefix = `${this.collectionName}/`;
    const docs = [];
    for (const [key, value] of this.db.values) {
      if (!key.startsWith(prefix)) continue;
      const matches = this.filters.every(({ field, operation, value: expected }) => {
        const actual = value[field];
        if (operation === 'array-contains') return Array.isArray(actual) && actual.includes(expected);
        if (operation === '==') return actual === expected;
        if (operation === '>=') return actual >= expected;
        if (operation === '<=') return actual <= expected;
        throw new Error(`UNSUPPORTED_QUERY_${operation}`);
      });
      if (!matches) continue;
      const id = key.slice(prefix.length);
      docs.push(new MemoryDocumentSnapshot(new MemoryDocument(this.db, this.collectionName, id), value));
    }
    docs.sort((a, b) => a.id.localeCompare(b.id));
    const selected = docs.filter((doc) => doc.id > this.afterId).slice(0, this.maximum);
    return { docs: selected, empty: selected.length === 0, size: selected.length };
  }
}

class MemoryCollection extends MemoryQuery {
  constructor(db, collectionName) { super(db, collectionName); }
  doc(id) { return new MemoryDocument(this.db, this.collectionName, String(id)); }
}

class MemoryFirestore {
  constructor() {
    this.values = new Map();
    this.queue = Promise.resolve();
  }
  key(ref) { return `${ref.collectionName}/${ref.id}`; }
  read(ref, values = this.values) { return clone(values.get(this.key(ref))); }
  write(ref, value, options, values = this.values) {
    const key = this.key(ref);
    values.set(key, options && options.merge ? { ...(values.get(key) || {}), ...clone(value) } : clone(value));
  }
  collection(name) { return new MemoryCollection(this, name); }
  async runTransaction(work) {
    let release;
    const previous = this.queue;
    this.queue = new Promise((resolve) => { release = resolve; });
    await previous;
    const working = new Map([...this.values].map(([key, value]) => [key, clone(value)]));
    const transaction = {
      get: async (ref) => new MemoryDocumentSnapshot(ref, this.read(ref, working)),
      set: (ref, value, options) => this.write(ref, value, options, working),
      create: (ref, value) => {
        const key = this.key(ref);
        if (working.has(key)) throw new Error('ALREADY_EXISTS');
        working.set(key, clone(value));
      },
      delete: (ref) => working.delete(this.key(ref))
    };
    try {
      const result = await work(transaction);
      this.values = working;
      return result;
    } finally {
      release();
    }
  }
}

const config = {
  enabled: true,
  canaryMode: false,
  canaryDiscordIds: new Set(),
  accountHmacKey: 'social-test-account-hmac-key-32-bytes-minimum',
  presence: { heartbeatMs: 45_000, staleMs: 120_000 },
  cover: {
    repoName: 'PROBBI/PROBBINE',
    branch: 'main',
    token: 'test-token',
    batchMs: 600_000,
    maxBytes: 256 * 1024,
    maxOperations: 75,
    cleanupGraceMs: 86_400_000,
    squashCommitThreshold: 5000,
    squashHistoryBytes: 2 * 1024 ** 3,
    autoSquash: false
  }
};

function identity(id, username) {
  return { id, username, displayName: username, avatarUrl: '', accountCreatedAt: '2020-01-01T00:00:00.000Z', tenantId: '0xolemon' };
}

test('reciprocal friend requests become one accepted relationship and block wins', async () => {
  const db = new MemoryFirestore();
  const events = new SocialEventHub({ now: () => 1000 });
  const service = new SocialService(db, config, events, { now: () => 1000 });
  const first = identity('111111111111111111', 'alpha');
  const second = identity('222222222222222222', 'beta');
  await service.ensureProfile(first);
  await service.ensureProfile(second);

  const outgoing = await service.mutateRelationship(first, second.id, 'request');
  assert.equal(outgoing.relationship, 'pending-out');
  const reciprocal = await service.mutateRelationship(second, first.id, 'request');
  assert.equal(reciprocal.relationship, 'friend');
  const replay = await service.mutateRelationship(second, first.id, 'accept');
  assert.equal(replay.relationship, 'friend');

  const blocked = await service.mutateRelationship(first, second.id, 'block');
  assert.equal(blocked.relationship, 'blocked');
  await assert.rejects(service.mutateRelationship(second, first.id, 'request'), { code: 'RELATIONSHIP_BLOCKED' });
});

test('relationship request IDs replay without emitting duplicate events', async () => {
  const db = new MemoryFirestore();
  const events = new SocialEventHub({ now: () => 1000 });
  const service = new SocialService(db, config, events, { now: () => 1000 });
  const first = identity('555555555555555555', 'delta');
  const second = identity('666666666666666666', 'epsilon');
  await service.ensureProfile(first);
  await service.ensureProfile(second);
  const requestId = '20000000-0000-4000-8000-000000000001';

  const initial = await service.mutateRelationship(first, second.id, 'request', requestId);
  const replay = await service.mutateRelationship(first, second.id, 'request', requestId);
  assert.deepEqual(replay, initial);
  assert.equal(events.events.filter((event) => event.type === 'relationship.updated').length, 1);
  await assert.rejects(
    service.mutateRelationship(first, second.id, 'block', requestId),
    { code: 'REQUEST_ID_REUSED' }
  );
});

test('relationship actions cannot delete a friendship through cancel, decline, or unblock', async () => {
  const db = new MemoryFirestore();
  const events = new SocialEventHub({ now: () => 1000 });
  const service = new SocialService(db, config, events, { now: () => 1000 });
  const first = identity('777777777777777777', 'friend-one');
  const second = identity('888888888888888888', 'friend-two');
  await service.ensureProfile(first);
  await service.ensureProfile(second);
  await service.mutateRelationship(first, second.id, 'request');
  await service.mutateRelationship(second, first.id, 'request');

  await assert.rejects(service.mutateRelationship(first, second.id, 'cancel'), { code: 'FRIEND_REQUEST_NOT_FOUND' });
  await assert.rejects(service.mutateRelationship(second, first.id, 'decline'), { code: 'FRIEND_REQUEST_NOT_FOUND' });
  const unchanged = await service.mutateRelationship(first, second.id, 'unblock');
  assert.equal(unchanged.relationship, 'friend');
  assert.equal((await service.getProfile(first, second.id)).relationship, 'friend');

  const removed = await service.mutateRelationship(first, second.id, 'remove');
  assert.equal(removed.relationship, 'none');
  assert.equal((await service.mutateRelationship(first, second.id, 'remove')).relationship, 'none');
});

test('presence aggregates sessions by playing, idle, online, then offline', async () => {
  let nowMs = 10_000;
  const db = new MemoryFirestore();
  const events = new SocialEventHub({ now: () => nowMs });
  const service = new SocialService(db, config, events, { now: () => nowMs });
  const user = identity('999999999999999999', 'presence-user');

  await service.heartbeat(user, { sessionId: 'desktop', state: 'online', activityLabel: 'Launcher' });
  await service.heartbeat(user, { sessionId: 'big-picture', state: 'playing', activityLabel: 'Playing', gameId: 'game-one' });
  await service.heartbeat(user, { sessionId: 'desktop', state: 'idle', activityLabel: 'Away' });
  assert.equal((await service.getProfile(user, user.id)).presence, 'playing');

  await service.heartbeat(user, { sessionId: 'big-picture', state: 'offline' });
  assert.equal((await service.getProfile(user, user.id)).presence, 'idle');

  await service.updateProfile(user, { appearOffline: true });
  await service.heartbeat(user, { sessionId: 'desktop', state: 'playing', activityLabel: 'Playing' });
  assert.equal((await service.getProfile(user, user.id)).presence, 'offline');

  nowMs += config.presence.staleMs + 1;
  await service.updateProfile(user, { appearOffline: false });
  assert.equal((await service.getProfile(user, user.id)).presence, 'offline');
});

test('prefix search uses a stable Firestore cursor without repeating users', async () => {
  const db = new MemoryFirestore();
  const service = new SocialService(db, config, new SocialEventHub(), { now: () => 1000 });
  const viewer = identity('100000000000000000', 'viewer');
  await service.ensureProfile(viewer);
  for (let index = 1; index <= 5; index += 1) {
    await service.ensureProfile(identity(`20000000000000000${index}`, `alpha-${index}`));
  }

  const first = await service.search(viewer, 'alpha', 2);
  const second = await service.search(viewer, 'alpha', 2, first.nextCursor);
  assert.equal(first.items.length, 2);
  assert.equal(second.items.length, 2);
  assert.ok(first.nextCursor);
  assert.equal(new Set([...first.items, ...second.items].map((item) => item.id)).size, 4);
});

test('stat events initialize numeric totals and are idempotent', async () => {
  const db = new MemoryFirestore();
  const service = new SocialService(db, config, new SocialEventHub(), { now: () => Date.UTC(2026, 7, 20) });
  const user = identity('333333333333333333', 'gamma');
  const event = {
    eventId: '10000000-0000-4000-8000-000000000001',
    kind: 'game-session',
    playMinutes: 35,
    onlineMinutes: 10,
    gamesPlayed: 1
  };
  assert.deepEqual(await service.recordStatEvent(user, event), { applied: true });
  assert.deepEqual(await service.recordStatEvent(user, event), { applied: false });
  const totals = (await db.collection('socialStatsTotals').doc(user.id).get()).data();
  assert.equal(totals.playMinutes, 35);
  assert.equal(totals.onlineMinutes, 10);
  assert.equal(totals.gamesPlayed, 1);
  assert.equal(Number.isNaN(totals.downloads), false);
});

test('event hub replays only visible events after Last-Event-ID', () => {
  let nowMs = 100;
  const hub = new SocialEventHub({ now: () => ++nowMs });
  const first = hub.publish('tenant', [], 'presence.updated', { userId: 'one' });
  hub.publish('tenant', ['two'], 'relationship.updated', { userId: 'two' });
  hub.publish('other', [], 'ignored', {});
  const received = [];
  const unsubscribe = hub.subscribe({ tenantId: 'tenant', userId: 'two', lastEventId: first.id, send: (event) => received.push(event.type) });
  unsubscribe();
  assert.deepEqual(received, ['relationship.updated']);
  assert.equal(hub.canReplay('tenant', first.id), true);
  assert.equal(hub.canReplay('tenant', 'event-from-before-render-restart'), false);
});

test('cover validation enforces WebP dimensions and publisher persists its queue', async () => {
  const buffer = await sharp({ create: { width: 1280, height: 360, channels: 3, background: '#17202a' } }).webp({ quality: 72 }).toBuffer();
  const hash = crypto.createHash('sha256').update(buffer).digest('hex');
  assert.equal(await validateCoverBuffer(buffer, 256 * 1024), hash);

  const db = new MemoryFirestore();
  const publisher = new HfCoverPublisher({
    config: config.cover,
    getTenantDb: () => db,
    eventHub: new SocialEventHub(),
    autoStart: false
  });
  await publisher.enqueue({
    tenantId: '0xolemon',
    userId: '444444444444444444',
    accountKey: 'a'.repeat(64),
    buffer,
    expectedHash: hash
  });
  assert.equal((await db.collection('socialCoverQueue').get()).size, 1);

  const restored = new HfCoverPublisher({
    config: config.cover,
    getTenantDb: () => db,
    eventHub: new SocialEventHub(),
    autoStart: false
  });
  await restored.hydrateTenant('0xolemon');
  assert.equal(restored.pending.size, 1);
});

test('account limiter rejects a burst beyond its fixed window', () => {
  const limiter = createAccountLimiter(() => 1000);
  const user = { id: '555555555555555555' };
  for (let index = 0; index < 120; index += 1) limiter(user);
  assert.throws(() => limiter(user), { code: 'SOCIAL_RATE_LIMITED' });
});
