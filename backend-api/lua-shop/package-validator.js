const crypto = require('crypto');
const yauzl = require('yauzl');
const { LuaShopError, assertAppId } = require('./quota');

const MAX_ARCHIVE_BYTES = 128 * 1024 * 1024;
const MAX_EXPANDED_BYTES = 512 * 1024 * 1024;
const MAX_ENTRIES = 4096;
const MAX_LUA_BYTES = 1024 * 1024;
const MANIFEST_MAGIC = 0x71f617d0;
const MANIFEST_PATTERN = /^([1-9]\d{0,9})_([1-9]\d{0,19})\.manifest$/;
const ALLOWED_CALLS = new Set([
  'addappid',
  'addtoken',
  'setmanifestid',
  'setappticket',
  'seteticket',
  'setstat',
  'forcedenuvo',
  'skipmanifestpin',
  'addprocess'
]);
const U64_MAX = 0xffffffffffffffffn;
const U32_MAX = 0xffffffffn;
const DECIMAL = '(?:0|[1-9]\\d{0,19})';
const QUOTED = '"(?:[^"\\\\\\u0000-\\u001f]|\\\\["\\\\nrt])*"';
const ADD_PROCESS_CALL = new RegExp(`^addProcess\\((${DECIMAL}), (${QUOTED})\\)$`);

function sha256(value) {
  return crypto.createHash('sha256').update(value).digest('hex');
}

function safePath(value) {
  if (typeof value !== 'string' || value.length === 0 || value.length > 300) return false;
  const normalized = value.replace(/\\/g, '/');
  if (normalized.startsWith('/') || /^[A-Za-z]:/.test(normalized)) return false;
  const path = normalized.endsWith('/') ? normalized.slice(0, -1) : normalized;
  if (!path) return false;
  return path.split('/').every((part) => part && part !== '.' && part !== '..');
}

function isSymlink(entry) {
  const unixMode = (entry.externalFileAttributes >>> 16) & 0xffff;
  return (unixMode & 0o170000) === 0o120000;
}

function readEntry(zipFile, entry, limit) {
  return new Promise((resolve, reject) => {
    zipFile.openReadStream(entry, (error, stream) => {
      if (error) return reject(error);
      const chunks = [];
      let size = 0;
      stream.on('data', (chunk) => {
        size += chunk.length;
        if (size > limit) stream.destroy(new Error('ZIP_ENTRY_TOO_LARGE'));
        else chunks.push(chunk);
      });
      stream.on('error', reject);
      stream.on('end', () => resolve(Buffer.concat(chunks)));
    });
  });
}

function openZip(buffer) {
  return new Promise((resolve, reject) => {
    yauzl.fromBuffer(buffer, {
      lazyEntries: true,
      decodeStrings: true,
      validateEntrySizes: true,
      strictFileNames: true
    }, (error, zipFile) => {
      if (error) reject(error);
      else resolve(zipFile);
    });
  });
}

function validDecimal(value, max = U64_MAX) {
  if (!new RegExp(`^${DECIMAL}$`).test(value)) return false;
  try {
    return BigInt(value) <= max;
  } catch {
    return false;
  }
}

function decodeCanonicalString(value) {
  if (!new RegExp(`^${QUOTED}$`).test(value)) return null;
  try {
    const decoded = JSON.parse(value);
    return typeof decoded === 'string' ? decoded : null;
  } catch {
    return null;
  }
}

function splitCanonicalArguments(raw) {
  const values = [];
  let start = 0;
  let quoted = false;
  let escaped = false;
  for (let index = 0; index < raw.length; index += 1) {
    const character = raw[index];
    if (escaped) {
      escaped = false;
      continue;
    }
    if (quoted && character === '\\') {
      escaped = true;
      continue;
    }
    if (character === '"') {
      quoted = !quoted;
      continue;
    }
    if (!quoted && character === ',' && raw[index + 1] === ' ') {
      values.push(raw.slice(start, index));
      start = index + 2;
      index += 1;
    }
  }
  if (quoted || escaped) return null;
  values.push(raw.slice(start));
  return values;
}

function inspectCanonicalLua(appid, buffer) {
  if (!Buffer.isBuffer(buffer) || buffer.length === 0 || buffer.length > MAX_LUA_BYTES) {
    throw new LuaShopError('PACKAGE_LUA_INVALID', 'Lua source is empty or too large.');
  }
  let source;
  try {
    source = new TextDecoder('utf-8', { fatal: true }).decode(buffer);
  } catch {
    throw new LuaShopError('PACKAGE_LUA_INVALID', 'Lua source is not valid UTF-8.');
  }
  if (source.includes('\u0000')) {
    throw new LuaShopError('PACKAGE_LUA_INVALID', 'Lua source is not valid UTF-8.');
  }
  if (source.includes('\r') || !source.endsWith('\n')) {
    throw new LuaShopError('PACKAGE_LUA_UNSUPPORTED', 'Lua source is not in canonical form.');
  }
  const lines = source.slice(0, -1).split('\n');
  if (lines.shift() !== '-- Canonical Lua package managed by 0xoLemon') {
    throw new LuaShopError('PACKAGE_LUA_UNSUPPORTED', 'Lua source is not a canonical launcher package.');
  }
  let rootRegistered = false;
  const manifestPins = new Map();
  for (const line of lines) {
    if (!line || line.trim() !== line) {
      throw new LuaShopError('PACKAGE_LUA_UNSUPPORTED', 'Lua source contains a non-canonical statement.');
    }
    const call = /^([A-Za-z][A-Za-z0-9_]*)\(/.exec(line);
    if (!call || !ALLOWED_CALLS.has(call[1].toLowerCase())) {
      throw new LuaShopError('PACKAGE_LUA_UNSUPPORTED', 'Lua source contains an unsupported statement.');
    }
    let match;
    if ((match = /^addappid\((\d+)(?:, (\d+)(?:, "([a-f0-9]{64})")?)?\)$/.exec(line))) {
      if (!validDecimal(match[1], U32_MAX) || (match[2] && !validDecimal(match[2]))) throw new Error('invalid');
      if (match[1] === String(appid)) rootRegistered = true;
      continue;
    }
    if ((match = /^addtoken\((\d+), "(\d+)"\)$/.exec(line))) {
      if (!validDecimal(match[1], U32_MAX) || !validDecimal(match[2])) throw new Error('invalid');
      continue;
    }
    if ((match = /^setManifestid\((\d+), "(\d+)"(?:, (\d+))?\)$/.exec(line))) {
      if (!validDecimal(match[1], U32_MAX) || !validDecimal(match[2]) || (match[3] && !validDecimal(match[3]))) throw new Error('invalid');
      const previous = manifestPins.get(match[1]);
      if (previous && previous !== match[2]) {
        throw new LuaShopError('PACKAGE_LUA_UNSUPPORTED', 'Lua source contains conflicting manifest pins.');
      }
      manifestPins.set(match[1], match[2]);
      continue;
    }
    if ((match = /^set(?:App|E)Ticket\((.*)\)$/.exec(line))) {
      const values = splitCanonicalArguments(match[1]);
      if (!values || values.length < 1 || values.length > 2 || !values.every((value) => validDecimal(value) || decodeCanonicalString(value) !== null)) throw new Error('invalid');
      continue;
    }
    if ((match = /^setStat\((\d+)(?:, "(\d+)")?\)$/.exec(line))) {
      if (!validDecimal(match[1], U32_MAX) || (match[2] && !validDecimal(match[2]))) throw new Error('invalid');
      continue;
    }
    if ((match = /^(?:forceDenuvo|skipManifestPin)\((\d+)\)$/.exec(line))) {
      if (!validDecimal(match[1], U32_MAX)) throw new Error('invalid');
      continue;
    }
    if ((match = ADD_PROCESS_CALL.exec(line))) {
      const executable = decodeCanonicalString(match[2]);
      if (!validDecimal(match[1], U32_MAX) || !executable || executable.length > 260 || /[\\/]/.test(executable) || !executable.toLowerCase().endsWith('.exe')) throw new Error('invalid');
      continue;
    }
    throw new LuaShopError('PACKAGE_LUA_UNSUPPORTED', 'Lua source contains a non-canonical statement.');
  }
  if (!rootRegistered) {
    throw new LuaShopError('PACKAGE_APPID_MISMATCH', 'Lua source belongs to another AppID.');
  }
  return { source, manifestPins };
}

function validateCanonicalLua(appid, buffer) {
  try {
    return inspectCanonicalLua(appid, buffer).source;
  } catch (error) {
    if (error instanceof LuaShopError) throw error;
    throw new LuaShopError('PACKAGE_LUA_UNSUPPORTED', 'Lua source contains invalid canonical arguments.');
  }
}

async function validateCanonicalPackage(buffer, expectedAppId, expectedRevision) {
  const appid = assertAppId(expectedAppId);
  if (!Buffer.isBuffer(buffer) || buffer.length === 0 || buffer.length > MAX_ARCHIVE_BYTES) {
    throw new LuaShopError('PACKAGE_TOO_LARGE', 'Community package exceeds the compressed size limit.', 413);
  }
  const packageHash = sha256(buffer);
  if (expectedRevision && expectedRevision.toLowerCase() !== packageHash) {
    throw new LuaShopError('PACKAGE_HASH_MISMATCH', 'Community package hash does not match its revision.');
  }
  let zipFile;
  try {
    zipFile = await openZip(buffer);
  } catch {
    throw new LuaShopError('PACKAGE_ZIP_INVALID', 'Community package is not a valid ZIP archive.');
  }
  const files = new Map();
  const seen = new Set();
  let entries = 0;
  let expandedBytes = 0;
  try {
    await new Promise((resolve, reject) => {
      zipFile.on('entry', async (entry) => {
        try {
          entries += 1;
          if (entries > MAX_ENTRIES) throw new Error('ZIP_TOO_MANY_ENTRIES');
          const name = entry.fileName.replace(/\\/g, '/');
          if (!safePath(name)) throw new Error('ZIP_UNSAFE_PATH');
          if (isSymlink(entry)) throw new Error('ZIP_SYMLINK');
          const normalized = name.toLowerCase();
          if (seen.has(normalized)) throw new Error('ZIP_DUPLICATE_PATH');
          seen.add(normalized);
          const isDirectory = /\/$/.test(name);
          if (!isDirectory) {
            expandedBytes += Number(entry.uncompressedSize || 0);
            if (expandedBytes > MAX_EXPANDED_BYTES) throw new Error('ZIP_EXPANDED_TOO_LARGE');
            const allowed = name === 'metadata.json' ||
              name === `lua/${appid}.lua` ||
              /^manifests\/[^/]+\.manifest$/i.test(name);
            if (!allowed) throw new Error('ZIP_UNEXPECTED_FILE');
            const data = await readEntry(zipFile, entry, MAX_EXPANDED_BYTES);
            files.set(name, data);
          }
          zipFile.readEntry();
        } catch (error) {
          reject(error);
        }
      });
      zipFile.once('end', resolve);
      zipFile.once('error', reject);
      zipFile.readEntry();
    });
  } catch (error) {
    zipFile.close();
    throw new LuaShopError('PACKAGE_ZIP_UNSAFE', `Community package failed ZIP safety validation: ${error.message}`);
  }

  const luaBuffer = files.get(`lua/${appid}.lua`);
  const metadataBuffer = files.get('metadata.json');
  if (!luaBuffer || !metadataBuffer) {
    throw new LuaShopError('PACKAGE_CONTENT_MISSING', 'Community package is missing Lua or metadata.');
  }
  let luaInfo;
  try {
    luaInfo = inspectCanonicalLua(appid, luaBuffer);
  } catch (error) {
    if (error instanceof LuaShopError) throw error;
    throw new LuaShopError('PACKAGE_LUA_UNSUPPORTED', 'Lua source contains invalid canonical arguments.');
  }
  const lua = luaInfo.source;
  let metadata;
  try {
    metadata = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(metadataBuffer));
  } catch {
    throw new LuaShopError('PACKAGE_METADATA_INVALID', 'Community package metadata is invalid.');
  }
  if (Number(metadata.appid) !== appid || Number(metadata.schemaVersion) !== 1) {
    throw new LuaShopError('PACKAGE_APPID_MISMATCH', 'Community package metadata belongs to another AppID.');
  }
  if (!Array.isArray(metadata.manifests) || metadata.manifests.length === 0) {
    throw new LuaShopError('PACKAGE_MANIFEST_MISSING', 'Community package has no depot manifests.');
  }
  const manifestMetadata = new Map(metadata.manifests.map((item) => [String(item.fileName || ''), item]));
  if (manifestMetadata.size !== metadata.manifests.length) {
    throw new LuaShopError('PACKAGE_MANIFEST_MISMATCH', 'Manifest metadata contains duplicate file names.');
  }
  const manifestFiles = [...files.entries()].filter(([name]) => name.startsWith('manifests/'));
  if (manifestFiles.length !== manifestMetadata.size) {
    throw new LuaShopError('PACKAGE_MANIFEST_MISMATCH', 'Manifest metadata does not match ZIP contents.');
  }
  const manifestDepots = new Set();
  const manifestIdentities = new Set();
  for (const [path, data] of manifestFiles) {
    const fileName = path.slice('manifests/'.length);
    const match = MANIFEST_PATTERN.exec(fileName);
    if (!match || data.length < 8 || data.readUInt32LE(0) !== MANIFEST_MAGIC) {
      throw new LuaShopError('PACKAGE_MANIFEST_INVALID', `Invalid depot manifest: ${fileName}`);
    }
    const item = manifestMetadata.get(fileName);
    if (!item || Number(item.depotId) !== Number(match[1]) || String(item.manifestGid) !== match[2]) {
      throw new LuaShopError('PACKAGE_MANIFEST_MISMATCH', `Manifest metadata mismatch: ${fileName}`);
    }
    if (manifestDepots.has(match[1])) {
      throw new LuaShopError('PACKAGE_MANIFEST_MISMATCH', `Package contains multiple manifests for depot ${match[1]}.`);
    }
    manifestDepots.add(match[1]);
    manifestIdentities.add(`${match[1]}:${match[2]}`);
    if (String(item.sha256 || '').toLowerCase() !== sha256(data) || Number(item.size) !== data.length) {
      throw new LuaShopError('PACKAGE_MANIFEST_HASH', `Manifest hash mismatch: ${fileName}`);
    }
  }
  for (const [depotId, manifestGid] of luaInfo.manifestPins) {
    if (!manifestIdentities.has(`${depotId}:${manifestGid}`)) {
      throw new LuaShopError('PACKAGE_MANIFEST_MISMATCH', `Lua pin has no matching manifest: ${depotId}_${manifestGid}.manifest`);
    }
  }
  return { appid, revision: packageHash, lua, metadata, sizeBytes: buffer.length };
}

module.exports = {
  MAX_ARCHIVE_BYTES,
  validateCanonicalLua,
  validateCanonicalPackage,
  safePath
};
