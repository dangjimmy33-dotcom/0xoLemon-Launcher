const { ActivationError } = require('./errors');

const DISCORD_EPOCH_MS = 1420070400000;

async function fetchJson(url, token, timeoutMs) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(url, {
      headers: { Authorization: `Bearer ${token}` },
      signal: controller.signal
    });
    if (response.status === 401) {
      throw new ActivationError('AUTH_REQUIRED', 'Discord authorization expired. Sign in again.', { httpStatus: 401 });
    }
    if (!response.ok) {
      throw new ActivationError('AUTH_UNAVAILABLE', 'Discord could not verify this account right now.', { httpStatus: 503 });
    }
    return response.json();
  } catch (error) {
    if (error instanceof ActivationError) throw error;
    throw new ActivationError('AUTH_UNAVAILABLE', 'Discord could not be reached. Try again later.', { httpStatus: 503 });
  } finally {
    clearTimeout(timer);
  }
}

class DiscordClient {
  constructor(config, now = () => Date.now()) {
    this.config = config;
    this.now = now;
  }

  async authorize(accessToken) {
    if (typeof accessToken !== 'string' || accessToken.length < 20 || accessToken.length > 4096) {
      throw new ActivationError('AUTH_REQUIRED', 'Discord authorization is required.', { httpStatus: 401 });
    }

    const user = await fetchJson(`${this.config.apiBase}/users/@me`, accessToken, this.config.timeoutMs);
    if (!user || !/^\d{15,22}$/.test(String(user.id || ''))) {
      throw new ActivationError('AUTH_REQUIRED', 'Discord returned an invalid account.', { httpStatus: 401 });
    }

    const member = await fetchJson(
      `${this.config.apiBase}/users/@me/guilds/${this.config.guildId}/member`,
      accessToken,
      this.config.timeoutMs
    );
    const roles = Array.isArray(member.roles) ? member.roles.map(String) : [];
    if (!roles.some((roleId) => this.config.allowedRoleIds.includes(roleId))) {
      throw new ActivationError('AUTH_FORBIDDEN', 'This Discord account does not have an activation role.', { httpStatus: 403 });
    }

    const accountCreatedAtMs = Number((BigInt(user.id) >> 22n) + BigInt(DISCORD_EPOCH_MS));
    const eligibleAt = accountCreatedAtMs + this.config.minimumAccountAgeMs;
    if (!Number.isFinite(accountCreatedAtMs) || eligibleAt > this.now()) {
      throw new ActivationError('ACCOUNT_TOO_NEW', 'This Discord account is not old enough for offline activation.', {
        httpStatus: 403,
        retryAt: eligibleAt
      });
    }

    return { id: String(user.id) };
  }
}

function bearerToken(req) {
  const header = req.get('authorization') || '';
  const match = /^Bearer\s+(.+)$/i.exec(header);
  return match ? match[1].trim() : '';
}

module.exports = { DiscordClient, bearerToken };

