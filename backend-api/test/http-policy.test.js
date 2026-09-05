const assert = require('node:assert/strict');
const test = require('node:test');
const express = require('express');
const { once } = require('node:events');
const { createHttpPolicy } = require('../http-policy');
const { requireSameOrigin } = require('../remote/security');

async function fixture(t, limits) {
  const config = { allowedOrigins: new Set(['https://launcher.example']) };
  const app = express();
  app.use(createHttpPolicy(config, limits));
  app.use((req, res) => res.json({ ok: true }));
  app.use((err, req, res, next) => res.status(err.status || 500).json({ error: err.message }));
  const server = app.listen(0, '127.0.0.1');
  await once(server, 'listening');
  t.after(() => new Promise(resolve => server.close(resolve)));
  const request = (path, origin, options = {}) => fetch(`http://127.0.0.1:${server.address().port}${path}`, {
    ...options, headers: { ...(origin ? { Origin: origin } : {}), ...options.headers }
  });
  return { request, config };
}

test('desktop release/dev work even with a web-only environment allowlist', async t => {
  const { request, config } = await fixture(t);
  for (const origin of ['http://tauri.localhost', 'http://localhost:14201', 'http://localhost:1420']) {
    for (const path of ['/health', '/api/0xolemon/catalog', '/api/0xolemon1/catalog', '/api/0xolemon/lua-shop/catalog/search', '/api/0xolemon/social/bootstrap']) {
      const r = await request(path, origin);
      assert.equal(r.status, 200);
      assert.equal(r.headers.get('access-control-allow-origin'), origin);
      assert.equal(r.headers.get('access-control-allow-credentials'), null);
    }
    assert.equal((await request('/api/0xolemon/auth/session', origin)).status, 403);
    assert.equal(requireSameOrigin({ get: () => origin }, config), false);
  }
  const web = await request('/api/0xolemon/auth/session', 'https://launcher.example');
  assert.equal(web.status, 200);
  assert.equal(web.headers.get('access-control-allow-credentials'), 'true');
  assert.equal((await request('/api/0xolemon/catalog')).status, 200);
});

test('unknown, null and lookalike origins fail closed with 403', async t => {
  const { request } = await fixture(t);
  for (const origin of ['null', 'https://evil.example', 'http://localhost:14201.evil.example', 'http://localhost:9999']) {
    const r = await request('/api/0xolemon/catalog', origin);
    assert.equal(r.status, 403);
    assert.equal(r.headers.get('access-control-allow-origin'), null);
    const payload = await r.json();
    assert.equal(payload.code, 'CORS_ORIGIN_DENIED');
    assert.match(payload.requestId, /^[a-f0-9-]{36}$/);
  }
});

test('denial/preflight do not consume quota and reads do not exhaust writes', async t => {
  const { request } = await fixture(t, { read: 2, write: 1 });
  for (let i = 0; i < 3; i++) {
    assert.equal((await request('/api/0xolemon/catalog', 'https://evil.example')).status, 403);
    assert.equal((await request('/api/0xolemon/catalog', 'http://localhost:14201', {
      method: 'OPTIONS', headers: { 'Access-Control-Request-Method': 'GET' }
    })).status, 204);
  }
  assert.equal((await request('/api/0xolemon/catalog', 'http://localhost:14201')).status, 200);
  assert.equal((await request('/api/0xolemon1/catalog', 'http://localhost:14201')).status, 200);
  const limited = await request('/api/0xolemon/lua-shop/catalog/search', 'http://localhost:14201');
  assert.equal(limited.status, 429);
  assert.ok(Number(limited.headers.get('retry-after')) > 0);
  assert.equal(limited.headers.get('access-control-allow-origin'), 'http://localhost:14201');
  assert.equal((await request('/api/0xolemon/social/profile', 'http://localhost:14201', { method: 'PUT' })).status, 200);
  assert.equal((await request('/api/0xolemon/social/profile', 'http://localhost:14201', { method: 'PUT' })).status, 429);
  assert.equal((await request('/health', 'http://localhost:14201')).status, 200);
});
