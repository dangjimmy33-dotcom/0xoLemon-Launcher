const crypto = require('crypto');

function decodeMasterKey(value) {
  if (!value) throw new Error('Activation encryption key is not configured');
  if (/^[a-f0-9]{64}$/i.test(value)) return Buffer.from(value, 'hex');
  const decoded = Buffer.from(value, 'base64');
  if (decoded.length === 32) return decoded;
  throw new Error('ACTIVATION_ENCRYPTION_KEY must be 32 bytes encoded as base64 or 64 hex characters');
}

class SecretCrypto {
  constructor(encodedMasterKey) {
    this.masterKey = decodeMasterKey(encodedMasterKey);
    this.accountHmacKey = Buffer.from(crypto.hkdfSync(
      'sha256',
      this.masterKey,
      Buffer.from('0xolemon-offline-activation'),
      Buffer.from('discord-account-hmac'),
      32
    ));
  }

  encrypt(value, context) {
    const iv = crypto.randomBytes(12);
    const cipher = crypto.createCipheriv('aes-256-gcm', this.masterKey, iv);
    cipher.setAAD(Buffer.from(context, 'utf8'));
    const encrypted = Buffer.concat([cipher.update(value, 'utf8'), cipher.final()]);
    const tag = cipher.getAuthTag();
    return `v1:${iv.toString('base64')}:${tag.toString('base64')}:${encrypted.toString('base64')}`;
  }

  decrypt(value, context) {
    const parts = String(value || '').split(':');
    if (parts.length !== 4 || parts[0] !== 'v1') throw new Error('Encrypted secret has an unsupported format');
    const iv = Buffer.from(parts[1], 'base64');
    const tag = Buffer.from(parts[2], 'base64');
    const encrypted = Buffer.from(parts[3], 'base64');
    const decipher = crypto.createDecipheriv('aes-256-gcm', this.masterKey, iv);
    decipher.setAAD(Buffer.from(context, 'utf8'));
    decipher.setAuthTag(tag);
    return Buffer.concat([decipher.update(encrypted), decipher.final()]).toString('utf8');
  }

  accountKey(discordId) {
    return crypto.createHmac('sha256', this.accountHmacKey).update(discordId).digest('hex');
  }

  ticketHash(ticket) {
    return crypto.createHash('sha256').update(ticket).digest('hex');
  }
}

function constantTimeKeyMatches(actual, expected) {
  const left = Buffer.from(String(actual || ''), 'utf8');
  const right = Buffer.from(String(expected || ''), 'utf8');
  return left.length > 0 && left.length === right.length && crypto.timingSafeEqual(left, right);
}

module.exports = { SecretCrypto, constantTimeKeyMatches };

