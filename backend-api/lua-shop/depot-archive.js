const crypto = require('node:crypto');
const yauzl = require('yauzl');
const { safePath } = require('./package-validator');
const { LuaShopError } = require('./quota');

const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const fail = code => { throw new LuaShopError(code, code, 422); };
const decimal = (value, max = 0xffffffffffffffffn) => {
  if (!/^[1-9]\d{0,19}$/.test(String(value)) || BigInt(value) > max) fail('ARCHIVE_INVALID_ID');
  return String(value);
};

async function readArchive(bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length > 128 * 1024 * 1024) fail('ARCHIVE_SIZE');
  const zip = await new Promise((resolve, reject) => yauzl.fromBuffer(bytes, { lazyEntries: true, strictFileNames: true }, (e, z) => e ? reject(e) : resolve(z)));
  const files = new Map();
  let expanded = 0, count = 0;
  try {
    await new Promise((resolve, reject) => {
      zip.on('error', reject); zip.on('end', resolve);
      zip.on('entry', entry => {
        if (!safePath(entry.fileName) || ++count > 4096 || files.has(entry.fileName.toLowerCase())
          || ((entry.externalFileAttributes >>> 16) & 0xf000) === 0xa000) return reject(new Error('ARCHIVE_UNSAFE_ENTRY'));
        if (entry.fileName.endsWith('/')) { zip.readEntry(); return; }
        expanded += entry.uncompressedSize;
        if (expanded > 512 * 1024 * 1024 || entry.uncompressedSize > 128 * 1024 * 1024) return reject(new Error('ARCHIVE_EXPANSION_LIMIT'));
        zip.openReadStream(entry, (error, stream) => {
          if (error) return reject(error);
          const chunks = []; let size = 0;
          stream.on('data', chunk => {
            size += chunk.length;
            if (size > entry.uncompressedSize) stream.destroy(new Error('ARCHIVE_SIZE_MISMATCH'));
            else chunks.push(chunk);
          });
          stream.on('error', reject);
          stream.on('end', () => { files.set(entry.fileName.toLowerCase(), Buffer.concat(chunks)); zip.readEntry(); });
        });
      });
      zip.readEntry();
    });
  } finally { zip.close(); }
  return files;
}

function inspectLua(bytes, appId) {
  if (bytes.length > 1024 * 1024) fail('ARCHIVE_LUA_SIZE');
  const bom = bytes.subarray(0, 3).equals(Buffer.from([239, 187, 191]));
  const text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
  if (text.includes('\0')) fail('ARCHIVE_LUA_ENCODING');
  const pins = new Map(), keys = new Set(), excluded = [];
  let root = false, sanitized = false, activeApps = 0;
  const lines = text.split(/(?<=\n)/);
  const output = [];
  for (const raw of lines) {
    const line = raw.replace(/^\uFEFF/, '').trim();
    if (!line || line.startsWith('--')) {
      // Public comments can also contain credentials. Remove such lines whole.
      if (/(?:token|ticket|password|steamid)\s*[:=]|7656119\d{10}/i.test(line)) { sanitized = true; continue; }
      const disabled = line.match(/^--\s*addappid\(\s*(\d+)/i);
      if (disabled) excluded.push({ appId: Number(disabled[1]), reason: 'providerExcluded' });
      output.push(raw); continue;
    }
    const code = line.replace(/\s*--.*$/, '').trim();
    const call = code.match(/^([a-z]+)\s*\((.*)\)\s*;?$/i);
    if (!call) fail('ARCHIVE_LUA_UNSUPPORTED');
    if (/^(addtoken|setappticket|seteticket|setstat)$/i.test(call[1])) { sanitized = true; continue; }
    let match;
    if ((match = code.match(/^addappid\(\s*(\d+)\s*(?:,\s*(\d+)\s*(?:,\s*["']([a-f\d]{64})["']\s*)?)?\)\s*;?$/i))) {
      const id = decimal(match[1], 0xffffffffn); root ||= id === String(appId); activeApps++;
      if (match[3]) keys.add(id);
    } else if ((match = code.match(/^setManifestid\(\s*(\d+)\s*,\s*["'](\d+)["']\s*(?:,\s*(\d+)\s*)?\)\s*;?$/i))) {
      const id = decimal(match[1], 0xffffffffn), gid = decimal(match[2]);
      if (pins.has(id)) fail('ARCHIVE_DUPLICATE_PIN');
      pins.set(id, gid);
    } else fail('ARCHIVE_LUA_UNSUPPORTED');
    const comment = raw.indexOf('--');
    if (comment >= 0 && /(?:token|ticket|password|steamid)\s*[:=]|7656119\d{10}/i.test(raw.slice(comment))) {
      sanitized = true;
      output.push(raw.slice(0, comment).trimEnd() + (raw.endsWith('\r\n') ? '\r\n' : raw.endsWith('\n') ? '\n' : ''));
    } else output.push(raw);
  }
  if (!root) fail('ARCHIVE_APPID_MISMATCH');
  const published = sanitized ? Buffer.from((bom ? '\uFEFF' : '') + output.join('').replace(/^\uFEFF/, '')) : bytes;
  return { pins, keys, excluded, activeApps, published, metadata: { originalSha256: sha(bytes), sha256: sha(published), sanitized, bom, encoding: 'utf-8', lineEnding: text.includes('\r\n') ? 'crlf' : 'lf' } };
}

function manifestIdentity(bytes) {
  if (bytes.length < 16 || bytes.readUInt32LE(0) !== 0x71f617d0) fail('ARCHIVE_MANIFEST_MAGIC');
  const start = 8 + bytes.readUInt32LE(4);
  if (start + 8 > bytes.length || bytes.readUInt32LE(start) !== 0x1f4812be) fail('ARCHIVE_MANIFEST_METADATA');
  const end = start + 8 + bytes.readUInt32LE(start + 4);
  if (end > bytes.length) fail('ARCHIVE_MANIFEST_TRUNCATED');
  let offset = start + 8;
  const varint = () => {
    let value = 0n;
    for (let shift = 0n; shift < 70n; shift += 7n) {
      if (offset >= end) fail('ARCHIVE_PROTOBUF_TRUNCATED');
      const byte = bytes[offset++]; value |= BigInt(byte & 127) << shift;
      if (!(byte & 128)) return value;
    }
    fail('ARCHIVE_PROTOBUF_INVALID');
  };
  const fields = new Map();
  while (offset < end) {
    const tag = Number(varint()), field = tag >>> 3, wire = tag & 7;
    if (wire === 0) fields.set(field, varint());
    else if (wire === 1) { if (offset + 8 > end) fail('ARCHIVE_PROTOBUF_TRUNCATED'); fields.set(field, bytes.readBigUInt64LE(offset)); offset += 8; }
    else if (wire === 2) { const length = Number(varint()); offset += length; }
    else if (wire === 5) offset += 4;
    else fail('ARCHIVE_PROTOBUF_INVALID');
    if (offset > end) fail('ARCHIVE_PROTOBUF_TRUNCATED');
  }
  return { depotId: Number(decimal(fields.get(1), 0xffffffffn)), manifestGid: decimal(fields.get(2)), contentSize: String(fields.get(5) || 0n) };
}

function steamSnapshot(appId, appinfo, observedAt = Date.now()) {
  const raw = appinfo.data?.[String(appId)] || appinfo;
  if (!raw.depots || !raw.common) fail('ARCHIVE_APPINFO_INVALID');
  const depots = Object.entries(raw.depots).filter(([id]) => /^\d+$/.test(id)).map(([id, depot]) => ({
    depotId: Number(id), os: depot.config?.oslist || null, language: depot.config?.language || null,
    dlcAppId: depot.dlcappid ? Number(depot.dlcappid) : null, sharedFromAppId: depot.depotfromapp ? Number(depot.depotfromapp) : null,
    manifests: Object.fromEntries(Object.entries(depot.manifests || {}).map(([branch, m]) => [branch, { gid: String(m.gid || m), size: String(m.size || 0) }])),
  }));
  const branches = Object.entries(raw.depots.branches || {}).map(([name, b]) => ({ name, buildId: String(b.buildid || ''), updatedAt: Number(b.timeupdated || 0) * 1000, passwordRequired: String(b.pwdrequired || '0') === '1' }));
  return { appId, title: raw.common.name || String(appId), observedAt, steamChangeNumber: String(raw._change_number || raw.change_number || ''), depots, branches };
}

function resolvePackage(appId, files, steam, { contemporaneous = false, branch = 'public', coverage = {} } = {}) {
  const luafiles = [...files].filter(([p]) => p.endsWith('.lua'));
  if (luafiles.length !== 1) fail('ARCHIVE_LUA_AMBIGUOUS');
  const lua = inspectLua(luafiles[0][1], appId);
  const manifests = [], missing = [], warnings = [];
  const seen = new Set();
  for (const [path, bytes] of files) {
    if (!path.endsWith('.manifest')) continue;
    const filename = path.split('/').pop();
    const match = filename.match(/^(\d+)_(\d+)\.manifest$/);
    const identity = manifestIdentity(bytes);
    if (!match || Number(match[1]) !== identity.depotId || match[2] !== identity.manifestGid) fail('ARCHIVE_MANIFEST_IDENTITY');
    if (seen.has(identity.depotId) || lua.pins.get(String(identity.depotId)) !== identity.manifestGid) fail('ARCHIVE_MANIFEST_PIN_MISMATCH');
    seen.add(identity.depotId);
    const depot = steam.depots.find(d => d.depotId === identity.depotId);
    manifests.push({ ...identity, fileName: filename, sha256: sha(bytes), sizeBytes: bytes.length, os: depot?.os, language: depot?.language, dlcAppId: depot?.dlcAppId, sharedFromAppId: depot?.sharedFromAppId });
  }
  for (const id of lua.pins.keys()) {
    if (!seen.has(Number(id))) missing.push({ depotId: Number(id), reason: 'manifestMissing' });
    if (!lua.keys.has(id)) missing.push({ depotId: Number(id), reason: 'keyMissing' });
  }
  const selected = steam.branches.find(b => b.name === branch);
  const exclusions = [];
  for (const depot of steam.depots) {
    if (seen.has(depot.depotId) || !depot.manifests[branch]) continue;
    const reason = depot.dlcAppId ? 'optionalDlc' : coverage.os && depot.os && !depot.os.split(',').includes(coverage.os) ? 'otherOs'
      : coverage.language && depot.language && ![coverage.language, 'english'].includes(depot.language) ? 'otherLanguage' : null;
    if (reason) exclusions.push({ depotId: depot.depotId, reason });
    else missing.push({ depotId: depot.depotId, reason: 'coverageMissing' });
  }
  const matching = steam.branches.filter(b => manifests.length && manifests.every(m => {
    const depot = steam.depots.find(d => d.depotId === m.depotId);
    return depot?.manifests[b.name]?.gid === m.manifestGid;
  }));
  const buildIdCandidates = [...new Set(matching.map(b => b.buildId).filter(Boolean))];
  const verified = contemporaneous && selected && !selected.passwordRequired && matching.some(b => b.name === branch) && !missing.length;
  const completeness = missing.length ? 'partial' : verified ? 'completeForCoverage' : 'unresolved';
  if (!verified) warnings.push('BUILD_ID_NOT_PROVEN');
  return { snapshot: { schemaVersion: 1, appId, title: steam.title, buildId: verified ? selected.buildId : null,
    buildIdCandidates, branchNames: verified ? matching.filter(b => b.buildId === selected.buildId).map(b => b.name) : [],
    steamChangeNumber: steam.steamChangeNumber, observedAt: steam.observedAt, source: 'community', provenance: 'communityContentVerified', completeness,
    contentFingerprint: sha([...lua.pins].sort(([a], [b]) => Number(a) - Number(b)).map(([id, gid]) => `${id}:${gid}`).join('\n')),
    coverage: { branch, os: coverage.os || null, language: coverage.language || null, exclusions, providerExcludedApps: lua.excluded, missing },
    lua: lua.metadata, manifests, warnings, backendValidatorVersion: 1 }, lua, files };
}

module.exports = { sha, readArchive, inspectLua, manifestIdentity, steamSnapshot, resolvePackage };
