const crypto = require('crypto');

const DEFAULT_ROLE_IDS = [
  '1492080961125355621',
  '1492130518869999737',
  '1492130703549267999',
  '1492131096937238588',
  '1510584783485403287',
  '1493617856238063669',
  '1492082591652909086',
  '1492568133486252182'
];

function boolEnv(name, fallback = false) {
  const value = String(process.env[name] || '').trim().toLowerCase();
  if (!value) return fallback;
  return value === '1' || value === 'true' || value === 'yes';
}

function intEnv(name, fallback, minimum = 1) {
  const value = Number.parseInt(process.env[name] || '', 10);
  return Number.isSafeInteger(value) && value >= minimum ? value : fallback;
}

function listEnv(name, fallback = []) {
  const value = String(process.env[name] || '').trim();
  return value ? value.split(',').map((item) => item.trim()).filter(Boolean) : fallback;
}

function decodeSecret(name, minimumBytes = 32) {
  const raw = String(process.env[name] || '').trim();
  if (!raw) return null;
  let bytes;
  try {
    bytes = /^[a-f0-9]+$/i.test(raw) && raw.length % 2 === 0
      ? Buffer.from(raw, 'hex')
      : Buffer.from(raw.replace(/-/g, '+').replace(/_/g, '/'), 'base64');
  } catch {
    return null;
  }
  return bytes.length >= minimumBytes ? bytes.subarray(0, 32) : null;
}

function loadRemoteConfig() {
  const sessionKey = decodeSecret('WEB_SESSION_ENCRYPTION_KEY');
  const accountHmacKey = String(
    process.env.REMOTE_ACCOUNT_HMAC_KEY || process.env.SOCIAL_ACCOUNT_HMAC_KEY || ''
  ).trim();
  const publicWebUrl = String(process.env.PUBLIC_WEB_URL || 'https://0xo-lemon-launcher.vercel.app').replace(/\/$/, '');
  const backendPublicUrl = String(process.env.PUBLIC_BASE_URL || 'https://zeroxolemon-launcher.onrender.com').replace(/\/$/, '');
  const secureCookies = String(process.env.NODE_ENV || '').toLowerCase() === 'production';

  return {
    webAuthEnabled: boolEnv('WEB_AUTH_ENABLED', false),
    remoteWebEnabled: boolEnv('REMOTE_WEB_ENABLED', false),
    canaryDiscordIds: new Set(listEnv('REMOTE_CANARY_DISCORD_IDS')),
    publicWebUrl,
    backendPublicUrl,
    allowedOrigins: new Set(listEnv('WEB_ALLOWED_ORIGINS', [publicWebUrl, 'http://localhost:5173', 'http://127.0.0.1:5173'])),
    secureCookies,
    sessionCookieName: secureCookies ? '__Host-0xolemon_session' : '0xolemon_session',
    oauthCookieName: secureCookies ? '__Host-0xolemon_oauth' : '0xolemon_oauth',
    csrfCookieName: '0xolemon_csrf',
    sessionKey,
    accountHmacKey,
    sessionTtlMs: intEnv('WEB_SESSION_DAYS', 30) * 24 * 60 * 60 * 1000,
    oauthTtlMs: 10 * 60 * 1000,
    legal: {
      termsVersion: String(process.env.LEGAL_TERMS_VERSION || '2026-08-25').trim(),
      privacyVersion: String(process.env.LEGAL_PRIVACY_VERSION || '2026-08-25').trim()
    },
    discord: {
      clientId: String(process.env.DISCORD_CLIENT_ID || process.env.OXO_DISCORD_CLIENT_ID || '').trim(),
      clientSecret: String(process.env.DISCORD_CLIENT_SECRET || '').trim(),
      redirectUri: String(process.env.DISCORD_WEB_REDIRECT_URI || `${publicWebUrl}/api/0xolemon/auth/discord/callback`).trim(),
      apiBase: 'https://discord.com/api/v10',
      authorizeUrl: 'https://discord.com/oauth2/authorize',
      tokenUrl: 'https://discord.com/api/v10/oauth2/token',
      guildId: process.env.DISCORD_REQUIRED_GUILD_ID || '1492076309323714570',
      allowedRoleIds: listEnv('DISCORD_ALLOWED_ROLE_IDS', DEFAULT_ROLE_IDS),
      minimumAccountAgeMs: intEnv('DISCORD_MINIMUM_ACCOUNT_AGE_DAYS', 7) * 24 * 60 * 60 * 1000,
      timeoutMs: intEnv('DISCORD_API_TIMEOUT_MS', 10000, 1000)
    }
  };
}

function assertRemoteConfig(config) {
  if (!config.webAuthEnabled && !config.remoteWebEnabled) return;
  const missing = [];
  if (!config.sessionKey) missing.push('WEB_SESSION_ENCRYPTION_KEY (base64/hex, at least 32 bytes)');
  if (config.accountHmacKey.length < 32) missing.push('REMOTE_ACCOUNT_HMAC_KEY or SOCIAL_ACCOUNT_HMAC_KEY');
  if (!config.discord.clientId) missing.push('DISCORD_CLIENT_ID');
  if (!config.discord.clientSecret) missing.push('DISCORD_CLIENT_SECRET');
  if (missing.length) {
    throw new Error(`Remote Web configuration is incomplete: ${missing.join(', ')}`);
  }
}

function accountKey(config, discordId) {
  return crypto.createHmac('sha256', config.accountHmacKey).update(String(discordId)).digest('hex');
}

module.exports = { accountKey, assertRemoteConfig, loadRemoteConfig };
