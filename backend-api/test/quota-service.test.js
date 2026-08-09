const assert = require('node:assert/strict');
const test = require('node:test');

const { ActivationQuotaService } = require('../activation/quota-service');

function clone(value) {
  return value == null ? value : structuredClone(value);
}

class MemoryStore {
  constructor() {
    this.collections = new Map();
    this.queue = Promise.resolve();
  }

  async transaction(work) {
    let release;
    const previous = this.queue;
    this.queue = new Promise((resolve) => { release = resolve; });
    await previous;
    const working = clone(Object.fromEntries(
      [...this.collections].map(([name, documents]) => [name, Object.fromEntries(documents)])
    ));
    const context = {
      get: async (collection, id) => clone(working[collection]?.[id] || null),
      set: (collection, id, value, merge = false) => {
        working[collection] ||= {};
        working[collection][id] = merge
          ? { ...(working[collection][id] || {}), ...clone(value) }
          : clone(value);
      }
    };
    try {
      const result = await work(context);
      this.collections = new Map(Object.entries(working).map(
        ([name, documents]) => [name, new Map(Object.entries(documents))]
      ));
      return result;
    } finally {
      release();
    }
  }
}

function fixture(startMs = Date.UTC(2026, 7, 9, 0, 0, 0)) {
  let nowMs = startMs;
  const config = {
    gameId: 'ea-sports-fc-26',
    capacity: 5,
    windowMs: 10 * 60 * 60 * 1000,
    cooldownMs: 96 * 60 * 60 * 1000,
    reservationMs: 3 * 60 * 1000,
    accountRateWindowMs: 15 * 60 * 1000,
    accountRateMax: 8
  };
  const service = new ActivationQuotaService(new MemoryStore(), config, () => nowMs);
  return {
    service,
    advance: (durationMs) => { nowMs += durationMs; }
  };
}

function request(index, accountKey = `account-${index}`) {
  return {
    requestId: `00000000-0000-4000-8000-${String(index).padStart(12, '0')}`,
    accountKey,
    ticketHash: `ticket-${index}`,
    launcherVersion: '2.0.42'
  };
}

test('six concurrent requests reserve exactly five global slots', async () => {
  const { service } = fixture();
  const results = await Promise.all(Array.from({ length: 6 }, (_, index) => (
    service.reserve(request(index + 1)).then(() => 'reserved').catch((error) => error.code)
  )));
  assert.equal(results.filter((result) => result === 'reserved').length, 5);
  assert.equal(results.filter((result) => result === 'NO_GLOBAL_SLOT').length, 1);
  assert.deepEqual(await service.status(), {
    capacity: 5,
    available: 0,
    inUse: 5,
    reservations: 5,
    pending: 0,
    nextAvailableAt: '2026-08-09T00:03:00.000Z',
    serverTime: '2026-08-09T00:00:00.000Z'
  });
});

test('one Discord account cannot reserve multiple global slots concurrently', async () => {
  const { service } = fixture();
  const results = await Promise.all([
    service.reserve(request(1, 'same-account')).then(() => 'reserved').catch((error) => error.code),
    service.reserve(request(2, 'same-account')).then(() => 'reserved').catch((error) => error.code)
  ]);
  assert.equal(results.filter((result) => result === 'reserved').length, 1);
  assert.equal(results.filter((result) => result === 'ACCOUNT_REQUEST_ACTIVE').length, 1);
  assert.equal((await service.status()).inUse, 1);
});

test('slots return independently in the rolling ten-hour window', async () => {
  const clock = fixture();
  for (let index = 1; index <= 5; index += 1) {
    const input = request(index);
    await clock.service.reserve(input);
    await clock.service.markStarted(input.requestId, input.accountKey);
    await clock.service.completeSuccess(input.requestId, input.accountKey, `encrypted-${index}`);
    if (index < 5) clock.advance(60 * 60 * 1000);
  }
  assert.equal((await clock.service.status()).available, 0);
  clock.advance(6 * 60 * 60 * 1000 + 1);
  assert.equal((await clock.service.status()).available, 1);
  clock.advance(60 * 60 * 1000);
  assert.equal((await clock.service.status()).available, 2);
});

test('successful request is idempotent and starts a 96-hour account cooldown', async () => {
  const clock = fixture();
  const { service } = clock;
  const first = request(1, 'same-account');
  await service.reserve(first);
  await service.markStarted(first.requestId, first.accountKey);
  await service.completeSuccess(first.requestId, first.accountKey, 'encrypted-token');

  const replay = await service.reserve(first);
  assert.deepEqual(replay, { kind: 'replay', encryptedToken: 'encrypted-token' });
  assert.equal((await service.status()).inUse, 1);

  await assert.rejects(
    service.reserve(request(2, 'same-account')),
    (error) => error.code === 'ACCOUNT_COOLDOWN' && error.retryAt === Date.UTC(2026, 7, 13, 0, 0, 0)
  );

  clock.advance(96 * 60 * 60 * 1000 + 1);
  const reusedAfterCooldown = await service.reserve(first);
  assert.equal(reusedAfterCooldown.kind, 'reserved');
});

test('a late response error cannot downgrade a committed success', async () => {
  const { service } = fixture();
  const input = request(1, 'stable-account');
  await service.reserve(input);
  await service.markStarted(input.requestId, input.accountKey);
  await service.completeSuccess(input.requestId, input.accountKey, 'encrypted-token');

  assert.deepEqual(
    await service.completeFailure(input.requestId, input.accountKey, true, 'INTERNAL_ERROR'),
    { ignored: true, completed: true }
  );
  assert.deepEqual(await service.reserve(input), {
    kind: 'replay',
    encryptedToken: 'encrypted-token'
  });
  assert.equal((await service.status()).inUse, 1);
});

test('definite failure releases a slot while uncertain outcome holds it', async () => {
  const { service } = fixture();
  const definite = request(1);
  await service.reserve(definite);
  await service.completeFailure(definite.requestId, definite.accountKey, false, 'EA_REJECTED');
  assert.equal((await service.status()).available, 5);

  const uncertain = request(2);
  await service.reserve(uncertain);
  await service.markStarted(uncertain.requestId, uncertain.accountKey);
  await service.completeFailure(uncertain.requestId, uncertain.accountKey, true, 'OUTCOME_UNCERTAIN');
  const status = await service.status();
  assert.equal(status.available, 4);
  assert.equal(status.pending, 1);
});
