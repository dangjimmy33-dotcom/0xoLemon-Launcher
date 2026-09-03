const crypto = require('crypto');

function base64url(bytes) {
  return Buffer.from(bytes).toString('base64url');
}

function randomToken(bytes = 32) {
  return base64url(crypto.randomBytes(bytes));
}

function hashToken(value) {
  return crypto.createHash('sha256').update(String(value)).digest('hex');
}

function sealJson(value, key) {
  if (!key) throw new Error('Session encryption is not configured');
  const iv = crypto.randomBytes(12);
  const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);
  const ciphertext = Buffer.concat([cipher.update(JSON.stringify(value), 'utf8'), cipher.final()]);
  return `${base64url(iv)}.${base64url(ciphertext)}.${base64url(cipher.getAuthTag())}`;
}

function openJson(value, key) {
  if (!key || typeof value !== 'string') return null;
  const parts = value.split('.');
  if (parts.length !== 3) return null;
  try {
    const iv = Buffer.from(parts[0], 'base64url');
    const ciphertext = Buffer.from(parts[1], 'base64url');
    const tag = Buffer.from(parts[2], 'base64url');
    const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
    decipher.setAuthTag(tag);
    const plaintext = Buffer.concat([decipher.update(ciphertext), decipher.final()]);
    return JSON.parse(plaintext.toString('utf8'));
  } catch {
    return null;
  }
}

function parseCookies(header) {
  const result = {};
  for (const pair of String(header || '').split(';')) {
    const index = pair.indexOf('=');
    if (index < 1) continue;
    const name = pair.slice(0, index).trim();
    const value = pair.slice(index + 1).trim();
    if (name) result[name] = decodeURIComponent(value);
  }
  return result;
}

function cookie(name, value, options = {}) {
  const parts = [`${name}=${encodeURIComponent(value)}`, `Path=${options.path || '/'}`];
  if (options.maxAge !== undefined) parts.push(`Max-Age=${Math.max(0, Math.floor(options.maxAge))}`);
  if (options.httpOnly) parts.push('HttpOnly');
  if (options.secure !== false) parts.push('Secure');
  parts.push(`SameSite=${options.sameSite || 'Lax'}`);
  return parts.join('; ');
}

function appendSetCookie(res, values) {
  const current = res.getHeader('Set-Cookie');
  const existing = Array.isArray(current) ? current : current ? [current] : [];
  res.setHeader('Set-Cookie', [...existing, ...values]);
}

function safeReturnPath(value) {
  if (typeof value !== 'string' || !value.startsWith('/') || value.startsWith('//')) return '/app';
  return value.slice(0, 256);
}

function requireSameOrigin(req, config) {
  const origin = String(req.get('origin') || '').replace(/\/$/, '');
  if (!origin) return true;
  return config.allowedOrigins.has(origin);
}

module.exports = {
  appendSetCookie,
  cookie,
  hashToken,
  openJson,
  parseCookies,
  randomToken,
  requireSameOrigin,
  safeReturnPath,
  sealJson
};
