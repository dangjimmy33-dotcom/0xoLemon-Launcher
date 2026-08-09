const crypto = require('crypto');
const { ActivationError } = require('./errors');

const DLF_KEY = Buffer.from([65, 50, 114, 45, 208, 130, 239, 176, 220, 100, 87, 197, 118, 104, 202, 9]);
const DLF_IV = Buffer.alloc(16);

function decryptDlfPayload(payload) {
  const decipher = crypto.createDecipheriv('aes-128-cbc', DLF_KEY, DLF_IV);
  return Buffer.concat([decipher.update(payload), decipher.final()]);
}

function decodedXml(buffer) {
  if (buffer.length >= 2 && buffer[0] === 0xfe && buffer[1] === 0xff) {
    const swapped = Buffer.alloc(buffer.length - 2);
    for (let index = 2; index + 1 < buffer.length; index += 2) {
      swapped[index - 2] = buffer[index + 1];
      swapped[index - 1] = buffer[index];
    }
    return swapped.toString('utf16le');
  }
  if (buffer.length >= 2 && buffer[0] === 0xff && buffer[1] === 0xfe) {
    return buffer.subarray(2).toString('utf16le');
  }
  return buffer.toString('utf8').replace(/^\uFEFF/, '');
}

function extractGameToken(data) {
  for (const candidate of [data, data.length > 0x41 ? data.subarray(0x41) : null]) {
    if (!candidate || candidate.length === 0 || candidate.length % 16 !== 0) continue;
    try {
      const xml = decodedXml(decryptDlfPayload(candidate));
      const match = /<(?:[A-Za-z0-9_-]+:)?GameToken\b[^>]*>([\s\S]*?)<\/(?:[A-Za-z0-9_-]+:)?GameToken>/i.exec(xml);
      if (match) {
        const token = match[1].replace(/\s+/g, '');
        if (token.length >= 16) return token;
      }
    } catch {
      // Some DLF responses have a 0x41-byte envelope; try the second candidate.
    }
  }
  throw new ActivationError('OUTCOME_UNCERTAIN', 'EA returned a license response that could not be decoded.', {
    httpStatus: 502,
    uncertain: true
  });
}

function parseTicket(ticket) {
  const parts = ticket.split('|');
  if (parts.length !== 3) {
    throw new ActivationError('INVALID_TICKET', 'The activation ticket has an invalid format.', { httpStatus: 400 });
  }
  const [requestToken, requestType, contentId] = parts;
  if (requestToken.length < 500 || requestToken.length > 24000 || !/^[A-Za-z0-9_=-]+$/.test(requestToken)) {
    throw new ActivationError('INVALID_TICKET', 'The activation ticket payload is invalid.', { httpStatus: 400 });
  }
  if (!/^\d{1,4}$/.test(requestType) || !/^\d{4,20}$/.test(contentId)) {
    throw new ActivationError('INVALID_TICKET', 'The activation ticket metadata is invalid.', { httpStatus: 400 });
  }
  return { requestToken, requestType, contentId };
}

class EaClient {
  constructor(config, db, secretCrypto, now = () => Date.now()) {
    this.config = config;
    this.db = db;
    this.secretCrypto = secretCrypto;
    this.now = now;
    this.accessToken = null;
    this.accessTokenExpiresAt = 0;
    this.refreshPromise = null;
    this.secretRef = db.collection('offlineActivationSecrets').doc('ea-sports-fc-26_ea');
  }

  async accessTokenForActivation() {
    if (this.accessToken && this.accessTokenExpiresAt > this.now() + 60000) return this.accessToken;
    if (!this.refreshPromise) {
      this.refreshPromise = this.refreshAccessToken().finally(() => { this.refreshPromise = null; });
    }
    return this.refreshPromise;
  }

  async loadRefreshToken() {
    const snapshot = await this.secretRef.get();
    if (snapshot.exists && snapshot.data().encryptedRefreshToken) {
      return this.secretCrypto.decrypt(snapshot.data().encryptedRefreshToken, 'ea-refresh-token');
    }
    if (!this.config.bootstrapRefreshToken) {
      throw new ActivationError('SERVICE_UNAVAILABLE', 'EA activation credentials are not configured.', { httpStatus: 503 });
    }
    return this.config.bootstrapRefreshToken;
  }

  async refreshAccessToken() {
    const refreshToken = await this.loadRefreshToken();
    const body = new URLSearchParams({
      grant_type: 'refresh_token',
      refresh_token: refreshToken,
      client_id: this.config.clientId,
      client_secret: this.config.clientSecret,
      token_format: 'JWS'
    });
    const response = await this.request(this.config.tokenUrl, {
      method: 'POST',
      headers: { 'content-type': 'application/x-www-form-urlencoded' },
      body
    }, false);

    let data;
    try { data = await response.json(); } catch { data = null; }
    if (!response.ok || !data || typeof data.access_token !== 'string') {
      throw new ActivationError('EA_AUTH_FAILED', 'EA service credentials could not be refreshed.', { httpStatus: 503 });
    }

    const rotatedRefreshToken = typeof data.refresh_token === 'string' ? data.refresh_token : refreshToken;
    await this.secretRef.set({
      encryptedRefreshToken: this.secretCrypto.encrypt(rotatedRefreshToken, 'ea-refresh-token'),
      updatedAtMs: this.now()
    }, { merge: true });
    this.accessToken = data.access_token;
    this.accessTokenExpiresAt = this.now() + Math.max(60, Number(data.expires_in || 3600)) * 1000;
    return this.accessToken;
  }

  async activate(ticket, accessToken) {
    const parsed = parseTicket(ticket);
    const query = new URLSearchParams({
      contentId: parsed.contentId,
      machineHash: this.config.machineHash,
      ea_eadmtoken: accessToken,
      requestToken: parsed.requestToken,
      requestType: parsed.requestType
    });
    const response = await this.request(`${this.config.licenseUrl}?${query}`, {
      headers: {
        'user-agent': 'EACTransaction',
        'x-requester-id': 'Origin Online Activation'
      }
    }, true);

    if (response.status >= 400 && response.status < 500) {
      throw new ActivationError('EA_REJECTED', 'EA rejected this activation ticket.', { httpStatus: 422 });
    }
    if (!response.ok) {
      throw new ActivationError('OUTCOME_UNCERTAIN', 'EA did not return a conclusive activation result.', {
        httpStatus: 503,
        uncertain: true
      });
    }
    const contentType = response.headers.get('content-type') || '';
    if (!contentType.toLowerCase().startsWith('application/octet-stream')) {
      throw new ActivationError('OUTCOME_UNCERTAIN', 'EA returned an unexpected activation response.', {
        httpStatus: 502,
        uncertain: true
      });
    }
    return extractGameToken(Buffer.from(await response.arrayBuffer()));
  }

  async request(url, options, outcomeMayBeUncertain) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.config.timeoutMs);
    try {
      return await fetch(url, { ...options, signal: controller.signal, redirect: 'error' });
    } catch {
      throw new ActivationError(
        outcomeMayBeUncertain ? 'OUTCOME_UNCERTAIN' : 'EA_AUTH_FAILED',
        outcomeMayBeUncertain
          ? 'Connection to EA was interrupted after activation started.'
          : 'EA authentication could not be reached.',
        { httpStatus: 503, uncertain: outcomeMayBeUncertain }
      );
    } finally {
      clearTimeout(timer);
    }
  }
}

module.exports = { EaClient, extractGameToken, parseTicket };

