const assert = require('node:assert/strict');
const crypto = require('crypto');
const test = require('node:test');

const { mergeIndex } = require('../lua-shop/hf-publisher');
const { loadCatalogPages, searchPublicStore } = require('../lua-shop/catalog');
const { validateCanonicalLua, validateCanonicalPackage } = require('../lua-shop/package-validator');
const { FirestoreLuaShopQuota, localDayKey, nextLocalMidnightMs } = require('../lua-shop/quota');

function clone(value) {
  return value == null ? value : structuredClone(value);
}

class MemorySnapshot {
  constructor(value) {
    this.value = clone(value);
    this.exists = value !== undefined;
  }
  data() { return clone(this.value); }
}

class MemoryDocument {
  constructor(db, collection, id) {
    this.db = db;
    this.collectionName = collection;
    this.id = id;
  }
  async get() { return new MemorySnapshot(this.db.read(this)); }
  async set(value, options) { this.db.write(this, value, options); }
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
    values.set(key, options && options.merge
      ? { ...(values.get(key) || {}), ...clone(value) }
      : clone(value));
  }
  collection(name) {
    return { doc: (id) => new MemoryDocument(this, name, id) };
  }
  async runTransaction(work) {
    let release;
    const previous = this.queue;
    this.queue = new Promise((resolve) => { release = resolve; });
    await previous;
    const working = new Map([...this.values].map(([key, value]) => [key, clone(value)]));
    const transaction = {
      get: async (ref) => new MemorySnapshot(this.read(ref, working)),
      set: (ref, value, options) => this.write(ref, value, options, working),
      create: (ref, value) => {
        const key = this.key(ref);
        if (working.has(key)) throw new Error('ALREADY_EXISTS');
        working.set(key, clone(value));
      },
      update: (ref, value) => {
        const key = this.key(ref);
        if (!working.has(key)) throw new Error('NOT_FOUND');
        working.set(key, { ...working.get(key), ...clone(value) });
      }
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

function requestId(index) {
  return `10000000-0000-4000-8000-${String(index).padStart(12, '0')}`;
}

function jsonResponse(payload, status = 200) {
  const body = JSON.stringify(payload);
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: { get: (name) => name.toLowerCase() === 'content-length' ? String(Buffer.byteLength(body)) : null },
    text: async () => body,
    json: async () => clone(payload)
  };
}

function steamSearchHtml(firstAppId, count) {
  return Array.from({ length: count }, (_, index) => {
    const appid = firstAppId + index;
    return `<a class="search_result_row" data-ds-appid="${appid}" data-ds-itemkey="App_${appid}"><span class="title">Game ${appid}</span></a>`;
  }).join('');
}

test('Steam IStore catalog follows its monotonic cursor and deduplicates AppIDs', async () => {
  const requested = [];
  const apps = await loadCatalogPages(async (url) => {
    requested.push(new URL(url).searchParams.get('last_appid'));
    if (requested.length === 1) {
      return jsonResponse({ response: {
        apps: [{ appid: 10, name: 'Alpha' }, { appid: 20, name: 'Beta' }],
        have_more_results: true,
        last_appid: 20
      } });
    }
    return jsonResponse({ response: {
      apps: [{ appid: 20, name: 'Beta newer' }, { appid: 30, name: 'Gamma' }],
      have_more_results: false,
      last_appid: 30
    } });
  }, 'server-secret');
  assert.deepEqual(requested, [null, '20']);
  assert.deepEqual(apps.map((entry) => [entry.appid, entry.name]), [
    [10, 'Alpha'], [20, 'Beta newer'], [30, 'Gamma']
  ]);
});

test('Steam IStore catalog rejects a non-advancing continuation cursor', async () => {
  await assert.rejects(
    loadCatalogPages(async () => jsonResponse({ response: {
      apps: [{ appid: 10, name: 'Alpha' }],
      have_more_results: true,
      last_appid: 0
    } }), 'server-secret'),
    /STEAM_CATALOG_CURSOR_STALLED/
  );
});

test('public Steam Store search is paged without a Web API key', async () => {
  const requestUrls = [];
  const result = await searchPublicStore('ark', 40, 20, async (url) => {
    const requestUrl = new URL(url);
    requestUrls.push(requestUrl);
    const start = Number(requestUrl.searchParams.get('start'));
    return jsonResponse({
      success: 1,
      start,
      total_count: 60,
      results_html: steamSearchHtml(start + 1, Math.min(50, 60 - start))
    });
  });
  assert.deepEqual(requestUrls.map((url) => url.searchParams.get('start')), ['0', '50']);
  assert.equal(requestUrls[0].searchParams.get('term'), 'ark');
  assert.equal(requestUrls[0].searchParams.has('key'), false);
  assert.equal(result.total, 60);
  assert.equal(result.nextOffset, null);
  assert.deepEqual(result.items.map((entry) => entry.appid), Array.from({ length: 20 }, (_, index) => 41 + index));
});

test('Lua Shop admits exactly ten concurrent new games and releases failed reservations', async () => {
  let nowMs = Date.UTC(2026, 7, 12, 2, 0, 0);
  const quota = new FirestoreLuaShopQuota(new MemoryFirestore(), { now: () => nowMs });
  const results = await Promise.all(Array.from({ length: 11 }, (_, index) => quota.reserve('account', {
    appid: 1000 + index,
    requestId: requestId(index + 1),
    timezone: 'Asia/Bangkok'
  }).then(() => 'reserved').catch((error) => error.code)));
  assert.equal(results.filter((value) => value === 'reserved').length, 10);
  assert.equal(results.filter((value) => value === 'DAILY_ADD_LIMIT').length, 1);

  await quota.settle('account', { appid: 1000, requestId: requestId(1) }, false);
  const replacement = await quota.reserve('account', {
    appid: 9999,
    requestId: requestId(12),
    timezone: 'Asia/Bangkok'
  });
  assert.equal(replacement.status, 'reserved');

  await quota.settle('account', { appid: 1001, requestId: requestId(2) }, true);
  const replay = await quota.settle('account', { appid: 1001, requestId: requestId(2) }, true);
  assert.equal(replay.status, 'completed');

  nowMs += 60_000;
  const reinstall = await quota.reserve('account', {
    appid: 1001,
    requestId: requestId(13),
    timezone: 'Asia/Bangkok'
  });
  assert.equal(reinstall.reinstall, true);
});

test('quota reset follows the account IANA timezone across local midnight', () => {
  const before = Date.UTC(2026, 7, 12, 16, 59, 0);
  assert.equal(localDayKey(before, 'Asia/Bangkok'), '2026-08-12');
  assert.equal(new Date(nextLocalMidnightMs(before, 'Asia/Bangkok')).toISOString(), '2026-08-12T17:00:00.000Z');
});

test('an expired reservation cannot overbook a slot that was reassigned', async () => {
  let nowMs = Date.UTC(2026, 7, 12, 2, 0, 0);
  const quota = new FirestoreLuaShopQuota(new MemoryFirestore(), {
    limit: 1,
    reservationMs: 60_000,
    now: () => nowMs
  });
  await quota.reserve('account', {
    appid: 2001,
    requestId: requestId(20),
    timezone: 'Asia/Bangkok'
  });
  nowMs += 61_000;
  await quota.reserve('account', {
    appid: 2002,
    requestId: requestId(21),
    timezone: 'Asia/Bangkok'
  });
  await assert.rejects(
    quota.settle('account', { appid: 2001, requestId: requestId(20) }, true),
    (error) => error.code === 'RESERVATION_EXPIRED'
  );
  const completed = await quota.settle(
    'account',
    { appid: 2002, requestId: requestId(21) },
    true
  );
  assert.equal(completed.status, 'completed');
});

test('two windows adding the same AppID consume one successful add', async () => {
  const quota = new FirestoreLuaShopQuota(new MemoryFirestore(), {
    now: () => Date.UTC(2026, 7, 12, 2, 0, 0)
  });
  await Promise.all([
    quota.reserve('account', {
      appid: 3001,
      requestId: requestId(30),
      timezone: 'Asia/Bangkok'
    }),
    quota.reserve('account', {
      appid: 3001,
      requestId: requestId(31),
      timezone: 'Asia/Bangkok'
    })
  ]);
  await Promise.all([
    quota.settle('account', { appid: 3001, requestId: requestId(30) }, true),
    quota.settle('account', { appid: 3001, requestId: requestId(31) }, true)
  ]);
  const status = await quota.status('account', 'Asia/Bangkok');
  assert.equal(status.used, 1);
  assert.equal(status.remaining, 9);
});

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function storedZip(files) {
  const locals = [];
  const centrals = [];
  let offset = 0;
  for (const [name, content] of files) {
    const nameBytes = Buffer.from(name, 'utf8');
    const data = Buffer.from(content);
    const checksum = crc32(data);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt32LE(checksum, 14);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(nameBytes.length, 26);
    locals.push(local, nameBytes, data);

    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt32LE(checksum, 16);
    central.writeUInt32LE(data.length, 20);
    central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(nameBytes.length, 28);
    central.writeUInt32LE(offset, 42);
    centrals.push(central, nameBytes);
    offset += local.length + nameBytes.length + data.length;
  }
  const centralData = Buffer.concat(centrals);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(centralData.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, centralData, end]);
}

test('canonical package validator accepts exact metadata and rejects executable Lua', async () => {
  const appid = 123;
  const manifest = Buffer.alloc(8);
  manifest.writeUInt32LE(0x71f617d0, 0);
  const manifestName = '456_789.manifest';
  const metadata = Buffer.from(JSON.stringify({
    schemaVersion: 1,
    appid,
    manifests: [{
      fileName: manifestName,
      depotId: 456,
      manifestGid: '789',
      sha256: crypto.createHash('sha256').update(manifest).digest('hex'),
      size: manifest.length
    }]
  }));
  const archive = storedZip([
    ['lua/', Buffer.alloc(0)],
    ['manifests/', Buffer.alloc(0)],
    [`lua/${appid}.lua`, Buffer.from([
      '-- Canonical Lua package managed by 0xoLemon',
      `addappid(${appid})`,
      `addappid(456, 1, "${'a'.repeat(64)}")`,
      `setManifestid(456, "789")`,
      ''
    ].join('\n'))],
    [`manifests/${manifestName}`, manifest],
    ['metadata.json', metadata]
  ]);
  const revision = crypto.createHash('sha256').update(archive).digest('hex');
  const result = await validateCanonicalPackage(archive, appid, revision);
  assert.equal(result.appid, appid);
  assert.equal(result.revision, revision);
  assert.throws(
    () => validateCanonicalLua(appid, Buffer.from(`addappid(${appid})\nos.execute("bad")\n`)),
    (error) => error.code === 'PACKAGE_LUA_UNSUPPORTED'
  );
});

test('community index is immutable by revision and contains no account identity', () => {
  const info = { appid: 123, revision: 'a'.repeat(64), sizeBytes: 42 };
  const index = mergeIndex(null, info, requestId(1));
  const repeated = mergeIndex(index, info, requestId(2));
  assert.equal(repeated.revisions.length, 1);
  assert.equal(JSON.stringify(repeated).includes('account'), false);
  assert.equal(JSON.stringify(repeated).includes(requestId(1)), false);
});
