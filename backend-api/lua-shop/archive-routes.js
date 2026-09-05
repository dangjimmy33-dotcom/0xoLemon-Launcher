const express = require('express');
const crypto = require('node:crypto');
const { assertAppId, LuaShopError } = require('./quota');
const { sha, readArchive, steamSnapshot, resolvePackage } = require('./depot-archive');
const publisher = require('./archive-publisher');

function createArchiveRouter({ getTenantDb, authenticateRequest, accountIpLimiter, accountIdentityLimiter, catalogLimiter }) {
  const router = express.Router({ mergeParams: true });
  const wrap = fn => async (req, res) => { try { await fn(req, res); } catch (error) {
    res.status(error instanceof LuaShopError ? error.httpStatus : 503).json({ code: error instanceof LuaShopError ? error.code : 'ARCHIVE_UNAVAILABLE', message: error instanceof LuaShopError ? error.message : 'Archive service is unavailable.', requestId: req.requestId });
  } };
  const db = req => getTenantDb(req.params.tenant);
  function service(req, res, next) {
    const expected = process.env.ARCHIVE_SERVICE_TOKEN || '';
    const actual = String(req.get('authorization') || '').replace(/^Bearer /, '');
    const expectedBytes = Buffer.from(expected), actualBytes = Buffer.from(actual);
    if (expectedBytes.length < 32 || actualBytes.length !== expectedBytes.length || !crypto.timingSafeEqual(actualBytes, expectedBytes)) return res.status(401).json({ code: 'ARCHIVE_SERVICE_AUTH_REQUIRED', requestId: req.requestId });
    next();
  }
  async function currentSteam(appId) {
    const response = await fetch(`https://api.steamcmd.net/v1/info/${appId}`, { signal: AbortSignal.timeout(30000) });
    if (!response.ok) throw new LuaShopError('STEAM_APPINFO_UNAVAILABLE', 'Steam metadata is unavailable.', 503);
    return steamSnapshot(appId, await response.json());
  }
  router.get('/apps/:appid', catalogLimiter, wrap(async (req, res) => {
    const appId = assertAppId(req.params.appid), apps = await publisher.read('Depotdownloader/index/apps.json');
    const app = apps?.apps?.find(a => a.appId === appId);
    res.json(app ? await publisher.read(`Depotdownloader/${app.folder}/${appId}/index.json`) : { schemaVersion: 1, appId, builds: [] });
  }));
  router.get('/apps/:appid/builds/:buildId', catalogLimiter, wrap(async (req, res) => {
    const appId = assertAppId(req.params.appid);
    if (!/^[1-9]\d{0,19}$/.test(req.params.buildId)) throw new LuaShopError('INVALID_BUILD_ID', 'Invalid BuildID.', 400);
    const apps = await publisher.read('Depotdownloader/index/apps.json'), app = apps?.apps?.find(a => a.appId === appId);
    const snapshot = app && await publisher.read(`Depotdownloader/${app.folder}/${appId}/builds/${req.params.buildId}/snapshot.json`);
    if (!snapshot) return res.status(404).json({ code: 'ARCHIVE_BUILD_NOT_FOUND', requestId: req.requestId });
    const index = await publisher.read(`Depotdownloader/${app.folder}/${appId}/index.json`);
    const entry = index?.builds?.find(b => b.buildId === req.params.buildId);
    res.json({ ...snapshot, package: entry?.packageSha256 ? { url: publisher.packageUrl(entry.path), sha256: entry.packageSha256, sizeBytes: entry.packageSizeBytes } : null });
  }));
  router.post('/candidates', accountIpLimiter, authenticateRequest, accountIdentityLimiter, express.raw({ type: 'application/zip', limit: '128mb' }), wrap(async (req, res) => {
    const appId = assertAppId(req.get('x-app-id'));
    if (!Buffer.isBuffer(req.body) || sha(req.body) !== req.get('x-content-sha256')) throw new LuaShopError('ARCHIVE_HASH_MISMATCH', 'Package hash mismatch.', 422);
    const files = await readArchive(req.body), steam = await currentSteam(appId);
    // Client timestamps/provenance are untrusted. Only service ingestion can bind a BuildID.
    const result = resolvePackage(appId, files, steam);
    if (result.snapshot.completeness === 'partial') throw new LuaShopError('ARCHIVE_PACKAGE_PARTIAL', 'Package is missing declared content.', 422);
    const candidateId = sha(`${appId}:${result.snapshot.contentFingerprint}:${sha(result.lua.published)}`);
    const ref = db(req).collection('depotArchiveCandidates').doc(candidateId);
    const receipt = await db(req).runTransaction(async tx => {
      const existing = await tx.get(ref);
      if (existing.exists) return { candidateId, state: existing.data().state };
      tx.set(ref, { candidateId, appId, fingerprint: result.snapshot.contentFingerprint, luaSha256: sha(result.lua.published), state: 'awaitingProviderProof', createdAt: Date.now() });
      return { candidateId, state: 'awaitingProviderProof' };
    });
    res.status(202).json(receipt);
  }));
  router.post('/resolve', accountIpLimiter, authenticateRequest, accountIdentityLimiter, express.raw({ type: 'application/zip', limit: '128mb' }), wrap(async (req, res) => {
    const appId = assertAppId(req.get('x-app-id'));
    if (!Buffer.isBuffer(req.body) || sha(req.body) !== req.get('x-content-sha256')) throw new LuaShopError('ARCHIVE_HASH_MISMATCH', 'Package hash mismatch.', 422);
    const files = await readArchive(req.body), steam = await currentSteam(appId);
    // Read-only inspection: a client's historical package never inherits today's BuildID.
    res.json(resolvePackage(appId, files, steam).snapshot);
  }));
  router.get('/candidates/:candidateId', catalogLimiter, authenticateRequest, wrap(async (req, res) => {
    if (!/^[a-f0-9]{64}$/.test(req.params.candidateId)) throw new LuaShopError('INVALID_CANDIDATE_ID', 'Invalid candidate ID.', 400);
    const doc = await db(req).collection('depotArchiveCandidates').doc(req.params.candidateId).get();
    if (!doc.exists) return res.status(404).json({ code: 'ARCHIVE_CANDIDATE_NOT_FOUND', requestId: req.requestId });
    const data = doc.data(); res.json({ candidateId: doc.id, state: data.state, appId: data.appId });
  }));
  router.get('/service/queue', service, wrap(async (req, res) => {
    const apps = await publisher.read('Depotdownloader/index/apps.json') || { apps: [] };
    const pending = await db(req).collection('depotArchiveCandidates').where('state', '==', 'awaitingProviderProof').limit(200).get();
    res.json({ appIds: [...new Set([...apps.apps.map(a => a.appId), ...pending.docs.map(d => d.data().appId)])] });
  }));
  router.get('/service/health', service, wrap(async (req, res) => {
    res.json({ configured: Boolean(process.env.HF_LUA_COMMUNITY_WRITE_TOKEN), branch: process.env.HF_DEPOT_ARCHIVE_BRANCH || 'archive-staging', validatorVersion: 1 });
  }));
  router.post('/reconcile', service, express.raw({ type: 'application/zip', limit: '128mb' }), wrap(async (req, res) => {
    const appId = assertAppId(req.get('x-app-id'));
    if (!Buffer.isBuffer(req.body) || sha(req.body) !== req.get('x-content-sha256')) throw new LuaShopError('ARCHIVE_HASH_MISMATCH', 'Package hash mismatch.', 422);
    const files = await readArchive(req.body);
    const evidence = files.get('steam-snapshot.json');
    if (!evidence) throw new LuaShopError('ARCHIVE_EVIDENCE_MISSING', 'Service capture evidence is missing.', 422);
    const capture = JSON.parse(evidence.toString('utf8'));
    if (capture.appId !== appId || !Number.isFinite(capture.capturedAt) || Math.abs(Date.now() - capture.capturedAt) > 10 * 60000) throw new LuaShopError('ARCHIVE_EVIDENCE_STALE', 'Capture evidence is stale.', 422);
    const steam = steamSnapshot(appId, capture.appinfo, capture.capturedAt);
    const fresh = await currentSteam(appId);
    const before = steam.branches.find(b => b.name === 'public'), after = fresh.branches.find(b => b.name === 'public');
    if (!before || before.buildId !== after?.buildId) throw new LuaShopError('ARCHIVE_STEAM_DRIFT', 'Steam changed during capture.', 409);
    const result = resolvePackage(appId, files, steam, { contemporaneous: true });
    const rechecked = resolvePackage(appId, files, fresh, { contemporaneous: true });
    if (rechecked.snapshot.completeness !== result.snapshot.completeness || rechecked.snapshot.buildId !== result.snapshot.buildId) throw new LuaShopError('ARCHIVE_STEAM_DRIFT', 'Steam coverage changed during capture.', 409);
    result.snapshot.provenance = 'hubcapVerified'; result.snapshot.source = '0xoLemon';
    const receipt = await publisher.publish(result);
    const candidateId = sha(`${appId}:${result.snapshot.contentFingerprint}:${sha(result.lua.published)}`);
    await db(req).collection('depotArchiveCandidates').doc(candidateId).set({ candidateId, appId, state: receipt.state, updatedAt: Date.now() }, { merge: true });
    await publisher.observe([steam]);
    res.json({ ...receipt, candidateId });
  }));
  router.post('/service/observe', service, wrap(async (req, res) => {
    const appId = assertAppId(req.body?.appId);
    const steam = await currentSteam(appId);
    const apps = await publisher.read('Depotdownloader/index/apps.json');
    const app = apps?.apps?.find(a => a.appId === appId);
    const index = app ? await publisher.read(`Depotdownloader/${app.folder}/${appId}/index.json`) : null;
    const buildId = steam.branches.find(b => b.name === 'public')?.buildId;
    await publisher.observe([steam]);
    res.json({ appId, buildId, archived: Boolean(index?.builds?.some(b => b.buildId === buildId)) });
  }));
  return router;
}
module.exports = { createArchiveRouter };
