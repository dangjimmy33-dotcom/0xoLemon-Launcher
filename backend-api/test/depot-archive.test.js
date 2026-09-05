const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { inspectLua, manifestIdentity, resolvePackage, steamSnapshot, readArchive } = require('../lua-shop/depot-archive');
const { archiveZip } = require('../lua-shop/archive-zip');
const { sameArtifact } = require('../lua-shop/archive-publisher');
const key = 'a'.repeat(64);
function manifest(id = 481, gid = 12) {
  const vi = value => { const bytes = []; let n = BigInt(value); do { let b = Number(n & 127n); n >>= 7n; if (n) b |= 128; bytes.push(b); } while (n); return bytes; };
  const metadata = Buffer.from([8, ...vi(id), 16, ...vi(gid), 40, 99]);
  const header = Buffer.alloc(16); header.writeUInt32LE(0x71f617d0); header.writeUInt32LE(0x1f4812be, 8); header.writeUInt32LE(metadata.length, 12);
  return Buffer.concat([header, metadata]);
}
const raw = Buffer.from(`-- provider\r\naddappid(480)\r\naddappid(481, 1, "${key}")\r\nsetManifestid(481, "12", 99)\r\n`);
const files = () => new Map([['480.lua', raw], ['481_12.manifest', manifest()]]);
const snapshot = () => steamSnapshot(480, { common: { name: 'Test' }, depots: { 481: { manifests: { public: { gid: '12' } } }, branches: { public: { buildid: '900', timeupdated: '100' } } } }, 123);
test('exact Lua CRLF and manifest binary identity are retained', () => {
  const result = inspectLua(raw, 480);
  assert.deepEqual(result.published, raw); assert.equal(result.metadata.lineEnding, 'crlf');
  assert.equal(result.pins.get('481'), '12'); assert.deepEqual(manifestIdentity(manifest()), { depotId: 481, manifestGid: '12', contentSize: '99' });
});
test('tickets and account comments are removed deterministically', () => {
  const bytes = Buffer.concat([raw, Buffer.from('addtoken(480, "123456")\r\n-- password=secret\r\nsetStat(480, "76561191234567890")\r\n')]);
  const result = inspectLua(bytes, 480);
  assert.equal(result.metadata.sanitized, true); assert.deepEqual(result.published, raw);
});

test('sanitization strips inline secrets while retaining BOM and depot declaration', () => {
  const input = Buffer.from('\uFEFF-- token=secret\r\naddappid(480) -- password=hidden\r\n');
  const result = inspectLua(input, 480);
  assert.deepEqual(result.published, Buffer.from('\uFEFFaddappid(480)\r\n'));
  assert.equal(result.metadata.bom, true);
});

test('immutable BuildID compares Lua and every manifest byte identity', () => {
  const a = resolvePackage(480, files(), snapshot(), { contemporaneous: true }).snapshot;
  assert.equal(sameArtifact(a, structuredClone(a)), true);
  const b = structuredClone(a); b.lua.sha256 = 'changed';
  assert.equal(sameArtifact(a, b), false);
  const c = structuredClone(a); c.manifests[0].sha256 = 'changed';
  assert.equal(sameArtifact(a, c), false);
});
test('only a contemporaneous complete branch can bind BuildID', () => {
  assert.equal(resolvePackage(480, files(), snapshot()).snapshot.completeness, 'unresolved');
  const verified = resolvePackage(480, files(), snapshot(), { contemporaneous: true }).snapshot;
  assert.equal(verified.buildId, '900'); assert.equal(verified.completeness, 'completeForCoverage');
  const missing = files(); missing.delete('481_12.manifest');
  assert.equal(resolvePackage(480, missing, snapshot(), { contemporaneous: true }).snapshot.completeness, 'partial');
});
test('renamed, conflicting and unreferenced manifests fail validation', () => {
  const mismatched = files(); mismatched.set('481_12.manifest', manifest(482));
  assert.throws(() => resolvePackage(480, mismatched, snapshot()), /ARCHIVE_MANIFEST_IDENTITY/);
  assert.throws(() => inspectLua(Buffer.concat([raw, Buffer.from('setManifestid(481, "13")\n')]), 480), /ARCHIVE_DUPLICATE_PIN/);
});
test('archive creation is deterministic and safely round trips', async () => {
  const a = archiveZip(files()), b = archiveZip(new Map([...files()].reverse()));
  assert.deepEqual(a, b);
  assert.deepEqual(await readArchive(a), files());
  const bad = archiveZip(new Map([['../escape.lua', raw]]));
  await assert.rejects(readArchive(bad));
});
test('supplied 2638890 fixture has 26 exact Lua to binary manifest matches', { skip: !fs.existsSync('E:/2638890/2638890.lua') }, () => {
  const lua = inspectLua(fs.readFileSync('E:/2638890/2638890.lua'), 2638890);
  assert.equal(lua.activeApps, 51); assert.equal(lua.pins.size, 26);
  const names = fs.readdirSync('E:/2638890').filter(name => name.endsWith('.manifest'));
  assert.equal(names.length, 26);
  for (const name of names) { const value = manifestIdentity(fs.readFileSync(`E:/2638890/${name}`)); assert.equal(lua.pins.get(String(value.depotId)), value.manifestGid); }
});
