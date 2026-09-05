const { archiveZip } = require('./archive-zip');
const { sha } = require('./depot-archive');
const { LuaShopError } = require('./quota');
const config = () => ({ repo: { type: 'dataset', name: process.env.HF_LUA_COMMUNITY_REPO || 'Immaking/Luas' }, branch: process.env.HF_DEPOT_ARCHIVE_BRANCH || 'archive-staging', accessToken: process.env.HF_LUA_COMMUNITY_WRITE_TOKEN });
async function read(path, parent, cfg = config()) {
  const r = await fetch(`https://huggingface.co/datasets/${cfg.repo.name}/resolve/${encodeURIComponent(parent || cfg.branch)}/${path.split('/').map(encodeURIComponent).join('/')}`, { signal: AbortSignal.timeout(30000), headers: cfg.accessToken ? { Authorization: `Bearer ${cfg.accessToken}` } : {} });
  if (r.status === 404) return null;
  if (!r.ok) throw new LuaShopError('ARCHIVE_READ_UNAVAILABLE', 'Archive is unavailable.', 503);
  return path.endsWith('.ndjson') ? r.text() : r.json();
}
async function commit(update) {
  const cfg = config();
  if (!cfg.accessToken) throw new LuaShopError('ARCHIVE_PUBLISH_NOT_CONFIGURED', 'Archive publishing is not configured.', 503);
  const hub = require('@huggingface/hub');
  for (let attempt = 0; attempt < 4; attempt++) {
    const revisionUrl = `https://huggingface.co/api/datasets/${cfg.repo.name}/revision/${cfg.branch}`;
    let r = await fetch(revisionUrl, { signal: AbortSignal.timeout(30000), headers: { Authorization: `Bearer ${cfg.accessToken}` } });
    if (r.status === 404 && cfg.branch === 'archive-staging') {
      try { await hub.createBranch({ repo: cfg.repo, branch: cfg.branch, revision: 'main', accessToken: cfg.accessToken }); }
      catch { /* A concurrent publisher may have created it; verify by reading. */ }
      r = await fetch(revisionUrl, { signal: AbortSignal.timeout(30000), headers: { Authorization: `Bearer ${cfg.accessToken}` } });
    }
    if (!r.ok) throw new LuaShopError('ARCHIVE_BRANCH_UNAVAILABLE', 'The configured archive branch is unavailable.', 503);
    const parent = (await r.json()).sha;
    const result = await update(parent, cfg);
    if (!result.operations.length) return result.receipt;
    try {
      await hub.commit({ ...cfg, parentCommit: parent, title: 'Update verified 0xoLemon depot archive', operations: result.operations });
      return result.receipt;
    } catch (error) {
      if (!/409|conflict|parent commit/i.test(String(error.message)) || attempt === 3) throw new LuaShopError('ARCHIVE_PUBLISH_FAILED', 'Archive commit failed.', 503);
    }
  }
}
const jsonFile = (path, value) => ({ operation: 'addOrUpdate', path, content: new Blob([JSON.stringify(value) + '\n']) });
// A depot/GID fingerprint does not cover Lua keys or binary corruption.
// Build immutability must include every published byte, not only Steam identity.
function sameArtifact(a, b) {
  const manifests = value => (value.manifests || []).map(m => `${m.depotId}:${m.manifestGid}:${m.sha256}:${m.sizeBytes}`).sort();
  return a.contentFingerprint === b.contentFingerprint && a.lua?.sha256 === b.lua?.sha256 && JSON.stringify(manifests(a)) === JSON.stringify(manifests(b));
}
async function publish(verified) {
  const snapshot = verified.snapshot;
  if (snapshot.provenance !== 'hubcapVerified' || snapshot.completeness !== 'completeForCoverage' || !snapshot.buildId) throw new LuaShopError('ARCHIVE_NOT_VERIFIED', 'Archive package is not verified.', 422);
  return commit(async (parent, cfg) => {
    const apps = await read('Depotdownloader/index/apps.json', parent, cfg) || { schemaVersion: 1, apps: [] };
    const current = apps.apps.find(a => a.appId === snapshot.appId);
    const safeName = snapshot.title.normalize('NFKC').replace(/[<>:"/\\|?*\x00-\x1f]/g, '_').replace(/[. ]+$/, '').slice(0, 100) || 'Game';
    const folder = current?.folder || `${safeName} (${snapshot.appId})`;
    const root = `Depotdownloader/${folder}/${snapshot.appId}`;
    const index = await read(`${root}/index.json`, parent, cfg) || { schemaVersion: 1, appId: snapshot.appId, builds: [] };
    const existing = index.builds.find(b => b.buildId === snapshot.buildId);
    if (existing) {
      if (!sameArtifact(existing, snapshot)) throw new LuaShopError('ARCHIVE_BUILD_CONFLICT', 'An immutable BuildID has different content.', 409);
      return { operations: [], receipt: { state: 'alreadyKnown', buildId: snapshot.buildId, path: existing.path } };
    }
    const path = `${root}/builds/${snapshot.buildId}`;
    const entries = new Map([[verified.lua.metadata.sanitized ? 'sanitized.lua' : 'provider.lua', verified.lua.published]]);
    for (const m of snapshot.manifests) {
      const bytes = [...verified.files].find(([p]) => p.split('/').pop() === m.fileName)?.[1];
      if (!bytes) throw new Error('ARCHIVE_MANIFEST_MISSING');
      entries.set(`manifests/${m.fileName}`, bytes);
    }
    entries.set('snapshot.json', Buffer.from(JSON.stringify(snapshot)));
    const operations = [...entries].map(([name, bytes]) => ({ operation: 'addOrUpdate', path: `${path}/${name}`, content: new Blob([bytes]) }));
    const packageBytes = archiveZip(entries);
    operations.push({ operation: 'addOrUpdate', path: `${path}/package.zip`, content: new Blob([packageBytes]) });
    index.builds.push({ ...snapshot, path, packageSha256: sha(packageBytes), packageSizeBytes: packageBytes.length, firstSeenAt: snapshot.observedAt });
    if (!current) apps.apps.push({ appId: snapshot.appId, folder, title: snapshot.title });
    operations.push(jsonFile(`${root}/index.json`, index), jsonFile('Depotdownloader/index/apps.json', apps));
    return { operations, receipt: { state: 'published', buildId: snapshot.buildId, path } };
  });
}
async function observe(snapshots) {
  return commit(async (parent, cfg) => {
    const path = `Depotdownloader/observations/${new Date().toISOString().slice(0, 10).replace(/-/g, '/')}.ndjson`;
    const previous = await read(path, parent, cfg) || '';
    const rows = previous.trim() ? previous.trim().split('\n').map(line => JSON.parse(line)) : [];
    const known = new Set(rows.map(r => `${r.appId}:${r.branch}:${r.buildId}`));
    for (const snapshot of snapshots) for (const branch of snapshot.branches.filter(b => b.name === 'public')) {
      const key = `${snapshot.appId}:${branch.name}:${branch.buildId}`;
      if (!known.has(key)) { rows.push({ appId: snapshot.appId, branch: branch.name, buildId: branch.buildId, observedAt: snapshot.observedAt }); known.add(key); }
    }
    const content = rows.map(r => JSON.stringify(r)).join('\n') + '\n';
    return { operations: content === previous ? [] : [{ operation: 'addOrUpdate', path, content: new Blob([content]) }], receipt: { state: 'observed', count: rows.length } };
  });
}
function packageUrl(path) {
  const cfg = config();
  return `https://huggingface.co/datasets/${cfg.repo.name}/resolve/${encodeURIComponent(cfg.branch)}/${path.split('/').map(encodeURIComponent).join('/')}/package.zip`;
}
module.exports = { read, publish, observe, sameArtifact, packageUrl };
