const { readArchive, sha } = require('../lua-shop/depot-archive');
const { archiveZip } = require('../lua-shop/archive-zip');

async function run() {
  const base = process.env.ARCHIVE_BACKEND_URL || 'https://zeroxolemon-launcher.onrender.com/api/0xolemon/lua-shop/archive';
  if (new URL(base).protocol !== 'https:') throw new Error('HTTPS_REQUIRED');
  if (!process.env.ARCHIVE_SERVICE_TOKEN || !process.env.HUBCAP_API_KEY) throw new Error('ARCHIVE_SECRETS_MISSING');
  const usage = await fetch('https://hubcapmanifest.com/api/v1/generate/usage', {
    signal: AbortSignal.timeout(30000), headers: { Authorization: `Bearer ${process.env.HUBCAP_API_KEY}` },
  });
  console.log(JSON.stringify({ providerAuthenticationStatus: usage.status }));
  if (!usage.ok) throw new Error(`PROVIDER_HTTP_${usage.status}`);
  const headers = { Authorization: `Bearer ${process.env.ARCHIVE_SERVICE_TOKEN}` };
  const request = async (path, options = {}) => {
    const r = await fetch(`${base}${path}`, { ...options, signal: AbortSignal.timeout(120000), headers: { ...headers, ...options.headers } });
    if (!r.ok) {
      const body = await r.json().catch(() => ({}));
      throw new Error(/^[A-Z_0-9]+$/.test(body.code) ? body.code : `ARCHIVE_HTTP_${r.status}`);
    }
    return r.json();
  };
  const health = await request('/service/health');
  console.log(JSON.stringify({ serviceConfigured: health.configured, targetBranch: health.branch, validatorVersion: health.validatorVersion }));
  if (!health.configured) throw new Error('ARCHIVE_PUBLISH_NOT_CONFIGURED');
  const queued = await request('/service/queue');
  const manual = String(process.env.ARCHIVE_APP_ID || '').trim();
  if (manual && !/^[1-9]\d{0,9}$/.test(manual)) throw new Error('INVALID_APP_ID');
  const appIds = manual ? [Number(manual)] : queued.appIds;
  const summary = { observed: 0, published: 0, deferred: [] };
  for (const appId of appIds) {
    try {
      const observation = await request('/service/observe', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ appId }) });
      summary.observed++;
      if (observation.archived) continue;
      const appResponse = await fetch(`https://api.steamcmd.net/v1/info/${appId}`, { signal: AbortSignal.timeout(30000) });
      if (!appResponse.ok) throw new Error(`STEAM_HTTP_${appResponse.status}`);
      const appinfo = await appResponse.json(), capturedAt = Date.now();
      // This endpoint provides the paired provider Lua and manifests. A failed
      // bundle is deferred intact; a different provider cannot supply its proof.
      const bundle = await fetch(`https://hubcapmanifest.com/api/v1/manifest/${appId}`, {
        signal: AbortSignal.timeout(240000), headers: { Authorization: `Bearer ${process.env.HUBCAP_API_KEY}` },
      });
      if (!bundle.ok) {
        summary.deferred.push({ appId, code: `PROVIDER_HTTP_${bundle.status}` });
        if (bundle.status === 429 || bundle.status === 401 || bundle.status === 403) break;
        continue;
      }
      const reader = bundle.body.getReader(); const chunks = []; let total = 0;
      for (;;) { const { done, value } = await reader.read(); if (done) break; total += value.length; if (total > 128 * 1024 * 1024) { await reader.cancel(); throw new Error('ARCHIVE_SIZE'); } chunks.push(value); }
      const entries = await readArchive(Buffer.concat(chunks));
      entries.set('steam-snapshot.json', Buffer.from(JSON.stringify({ appId, capturedAt, appinfo })));
      const payload = archiveZip(entries);
      await request('/reconcile', { method: 'POST', headers: { 'Content-Type': 'application/zip', 'X-App-Id': String(appId), 'X-Content-Sha256': sha(payload) }, body: payload });
      summary.published++;
    } catch (error) {
      summary.deferred.push({ appId, code: /^[A-Z_0-9]+$/.test(error.message) ? error.message : 'RECONCILE_FAILED' });
    }
  }
  console.log(JSON.stringify(summary));
  if (summary.deferred.length) process.exitCode = 1;
}
if (require.main === module) run().catch(error => { console.error(/^[A-Z_0-9]+$/.test(error.message) ? error.message : 'ARCHIVE_RECONCILE_FAILED'); process.exitCode = 1; });
module.exports = { run };
