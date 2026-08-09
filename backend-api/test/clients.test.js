const assert = require('node:assert/strict');
const crypto = require('crypto');
const test = require('node:test');

const { DiscordClient } = require('../activation/discord-client');
const { EaClient, extractGameToken } = require('../activation/ea-client');
const { SecretCrypto } = require('../activation/secret-crypto');

const DLF_KEY = Buffer.from([65, 50, 114, 45, 208, 130, 239, 176, 220, 100, 87, 197, 118, 104, 202, 9]);

function encryptedDlf(token) {
  const cipher = crypto.createCipheriv('aes-128-cbc', DLF_KEY, Buffer.alloc(16));
  const xml = Buffer.from(`<License xmlns="http://ea.com/license"><GameToken>${token}</GameToken></License>`, 'utf8');
  return Buffer.concat([cipher.update(xml), cipher.final()]);
}

test('Discord authorization verifies account, guild role, and account age', async (context) => {
  const originalFetch = global.fetch;
  context.after(() => { global.fetch = originalFetch; });
  const oldAccountId = String((BigInt(Date.UTC(2020, 0, 1) - 1420070400000) << 22n));
  global.fetch = async (url) => new Response(JSON.stringify(
    url.endsWith('/users/@me') ? { id: oldAccountId } : { roles: ['allowed-role'] }
  ), { status: 200, headers: { 'content-type': 'application/json' } });
  const client = new DiscordClient({
    apiBase: 'https://discord.test',
    guildId: 'guild',
    allowedRoleIds: ['allowed-role'],
    minimumAccountAgeMs: 7 * 24 * 60 * 60 * 1000,
    timeoutMs: 1000
  }, () => Date.UTC(2026, 7, 9));
  assert.deepEqual(await client.authorize('x'.repeat(40)), { id: oldAccountId });
});

test('EA client refreshes credentials and decodes the license without real network calls', async (context) => {
  const originalFetch = global.fetch;
  context.after(() => { global.fetch = originalFetch; });
  const token = 'generated-game-token';
  let storedSecret = null;
  const db = {
    collection: () => ({
      doc: () => ({
        get: async () => ({ exists: Boolean(storedSecret), data: () => storedSecret }),
        set: async (value) => { storedSecret = { ...(storedSecret || {}), ...value }; }
      })
    })
  };
  global.fetch = async (url) => {
    if (String(url).includes('/connect/token')) {
      return new Response(JSON.stringify({ access_token: 'access', refresh_token: 'rotated', expires_in: 3600 }), {
        status: 200,
        headers: { 'content-type': 'application/json' }
      });
    }
    return new Response(encryptedDlf(token), {
      status: 200,
      headers: { 'content-type': 'application/octet-stream' }
    });
  };
  const secretCrypto = new SecretCrypto(Buffer.alloc(32, 7).toString('base64'));
  const client = new EaClient({
    clientId: 'JUNO_PC_CLIENT',
    clientSecret: 'secret',
    machineHash: 'machine',
    bootstrapRefreshToken: 'bootstrap',
    tokenUrl: 'https://ea.test/connect/token',
    licenseUrl: 'https://ea.test/licenses',
    timeoutMs: 1000
  }, db, secretCrypto, () => Date.UTC(2026, 7, 9));
  const accessToken = await client.accessTokenForActivation();
  const ticket = `${'A'.repeat(500)}|0|16425677`;
  assert.equal(await client.activate(ticket, accessToken), token);
  assert.equal(secretCrypto.decrypt(storedSecret.encryptedRefreshToken, 'ea-refresh-token'), 'rotated');
  assert.equal(extractGameToken(encryptedDlf(token)), token);
});

test('AES-GCM storage is context-bound and account IDs are HMACed', () => {
  const secretCrypto = new SecretCrypto(Buffer.alloc(32, 11).toString('base64'));
  const encrypted = secretCrypto.encrypt('sensitive-token', 'request-1');
  assert.equal(secretCrypto.decrypt(encrypted, 'request-1'), 'sensitive-token');
  assert.throws(() => secretCrypto.decrypt(encrypted, 'request-2'));
  assert.match(secretCrypto.accountKey('123456789012345678'), /^[a-f0-9]{64}$/);
  assert.ok(!secretCrypto.accountKey('123456789012345678').includes('123456789012345678'));
});

