const crypto = require('crypto');

const REQUEST_ID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const TIMEZONE_PATTERN = /^[A-Za-z0-9_+\-/]{1,64}$/;

class LuaShopError extends Error {
  constructor(code, message, httpStatus = 400) {
    super(message);
    this.code = code;
    this.httpStatus = httpStatus;
  }
}

function assertAppId(value) {
  const appid = Number(value);
  if (!Number.isSafeInteger(appid) || appid <= 0 || appid > 0xffffffff) {
    throw new LuaShopError('INVALID_APPID', 'AppID is invalid.');
  }
  return appid;
}

function assertRequestId(value) {
  if (typeof value !== 'string' || !REQUEST_ID_PATTERN.test(value)) {
    throw new LuaShopError('INVALID_REQUEST_ID', 'requestId must be a UUID.');
  }
  return value.toLowerCase();
}

function assertTimezone(value) {
  if (typeof value !== 'string' || !TIMEZONE_PATTERN.test(value)) {
    throw new LuaShopError('INVALID_TIMEZONE', 'Timezone must be an IANA timezone.');
  }
  try {
    new Intl.DateTimeFormat('en-US', { timeZone: value }).format(new Date());
  } catch {
    throw new LuaShopError('INVALID_TIMEZONE', 'Timezone must be an IANA timezone.');
  }
  return value;
}

function localDayKey(timestampMs, timezone) {
  const parts = new Intl.DateTimeFormat('en-CA', {
    timeZone: timezone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit'
  }).formatToParts(new Date(timestampMs));
  const values = Object.fromEntries(parts.map((part) => [part.type, part.value]));
  return `${values.year}-${values.month}-${values.day}`;
}

function nextLocalMidnightMs(timestampMs, timezone) {
  const currentDay = localDayKey(timestampMs, timezone);
  let low = timestampMs;
  let high = timestampMs + (36 * 60 * 60 * 1000);
  while (high - low > 1) {
    const middle = Math.floor((low + high) / 2);
    if (localDayKey(middle, timezone) === currentDay) low = middle;
    else high = middle;
  }
  return high;
}

function hashDocumentId(value) {
  return crypto.createHash('sha256').update(value).digest('hex');
}

function pruneReservations(value, nowMs) {
  const reservations = value && typeof value === 'object' ? value : {};
  return Object.fromEntries(Object.entries(reservations).filter(([, reservation]) => (
    reservation && Number(reservation.expiresAtMs) > nowMs
  )));
}

class FirestoreLuaShopQuota {
  constructor(db, options = {}) {
    this.db = db;
    this.limit = options.limit || 10;
    this.reservationMs = options.reservationMs || (15 * 60 * 1000);
    this.timezoneLockMs = options.timezoneLockMs || (30 * 24 * 60 * 60 * 1000);
    this.now = options.now || (() => Date.now());
  }

  refs(accountKey, requestId, appid) {
    return {
      account: this.db.collection('luaShopQuota').doc(accountKey),
      request: this.db.collection('luaShopAddRequests').doc(hashDocumentId(`${accountKey}:${requestId}`)),
      installed: this.db.collection('luaShopInstalledApps').doc(hashDocumentId(`${accountKey}:${appid}`))
    };
  }

  normalizeAccount(data, requestedTimezone, nowMs) {
    const current = data || {};
    const locked = Number(current.timezoneLockedUntilMs || 0) > nowMs;
    const timezone = locked && current.timezone ? current.timezone : requestedTimezone;
    const dayKey = localDayKey(nowMs, timezone);
    return {
      timezone,
      timezoneLockedUntilMs: locked
        ? Number(current.timezoneLockedUntilMs)
        : nowMs + this.timezoneLockMs,
      dayKey,
      used: current.dayKey === dayKey ? Number(current.used || 0) : 0,
      reservations: pruneReservations(current.dayKey === dayKey ? current.reservations : {}, nowMs)
    };
  }

  publicState(account, nowMs) {
    const active = Object.keys(account.reservations || {}).length;
    return {
      limit: this.limit,
      used: account.used,
      remaining: Math.max(0, this.limit - account.used - active),
      activeReservations: active,
      resetAt: new Date(nextLocalMidnightMs(nowMs, account.timezone)).toISOString(),
      serverTime: new Date(nowMs).toISOString(),
      timezone: account.timezone,
      available: account.used + active < this.limit
    };
  }

  async status(accountKey, requestedTimezone = 'UTC') {
    const nowMs = this.now();
    const timezone = assertTimezone(requestedTimezone);
    const accountRef = this.db.collection('luaShopQuota').doc(accountKey);
    const snapshot = await accountRef.get();
    const account = this.normalizeAccount(snapshot.exists ? snapshot.data() : null, timezone, nowMs);
    return this.publicState(account, nowMs);
  }

  async reserve(accountKey, input) {
    const appid = assertAppId(input.appid);
    const requestId = assertRequestId(input.requestId);
    const requestedTimezone = assertTimezone(input.timezone || 'UTC');
    const nowMs = this.now();
    const refs = this.refs(accountKey, requestId, appid);
    return this.db.runTransaction(async (transaction) => {
      const [requestSnapshot, accountSnapshot, installedSnapshot] = await Promise.all([
        transaction.get(refs.request),
        transaction.get(refs.account),
        transaction.get(refs.installed)
      ]);
      if (requestSnapshot.exists) {
        const existing = requestSnapshot.data();
        if (existing.accountKey !== accountKey || Number(existing.appid) !== appid) {
          throw new LuaShopError('IDEMPOTENCY_CONFLICT', 'requestId is already used for another add.', 409);
        }
        if (existing.status === 'completed') {
          const account = this.normalizeAccount(
            accountSnapshot.exists ? accountSnapshot.data() : null,
            requestedTimezone,
            nowMs
          );
          return { status: 'completed', requestId, reinstall: Boolean(existing.reinstall), quota: this.publicState(account, nowMs) };
        }
        if (existing.status === 'reserved' && Number(existing.expiresAtMs) > nowMs) {
          const account = this.normalizeAccount(
            accountSnapshot.exists ? accountSnapshot.data() : null,
            requestedTimezone,
            nowMs
          );
          return { status: 'reserved', requestId, reinstall: Boolean(existing.reinstall), quota: this.publicState(account, nowMs) };
        }
        throw new LuaShopError('REQUEST_NOT_REUSABLE', 'This add request has already ended.', 409);
      }

      const account = this.normalizeAccount(
        accountSnapshot.exists ? accountSnapshot.data() : null,
        requestedTimezone,
        nowMs
      );
      const reinstall = installedSnapshot.exists;
      const active = Object.keys(account.reservations).length;
      if (!reinstall && account.used + active >= this.limit) {
        throw new LuaShopError('DAILY_ADD_LIMIT', 'Daily Lua Shop add limit reached.', 429);
      }
      const expiresAtMs = nowMs + this.reservationMs;
      if (!reinstall) {
        account.reservations[requestId] = { appid, expiresAtMs };
      }
      transaction.set(refs.account, {
        ...account,
        updatedAtMs: nowMs
      }, { merge: true });
      transaction.create(refs.request, {
        accountKey,
        appid,
        requestId,
        status: 'reserved',
        reinstall,
        createdAtMs: nowMs,
        expiresAtMs
      });
      return { status: 'reserved', requestId, reinstall, quota: this.publicState(account, nowMs) };
    });
  }

  async settle(accountKey, input, completed) {
    const appid = assertAppId(input.appid);
    const requestId = assertRequestId(input.requestId);
    const nowMs = this.now();
    const refs = this.refs(accountKey, requestId, appid);
    const result = await this.db.runTransaction(async (transaction) => {
      const [requestSnapshot, accountSnapshot, installedSnapshot] = await Promise.all([
        transaction.get(refs.request),
        transaction.get(refs.account),
        transaction.get(refs.installed)
      ]);
      if (!requestSnapshot.exists) {
        throw new LuaShopError('REQUEST_NOT_FOUND', 'Lua add reservation was not found.', 404);
      }
      const request = requestSnapshot.data();
      if (request.accountKey !== accountKey || Number(request.appid) !== appid) {
        throw new LuaShopError('IDEMPOTENCY_CONFLICT', 'requestId belongs to another add.', 409);
      }
      if (request.status === 'completed') return { status: 'completed', requestId };
      if (request.status === 'failed') return { status: 'failed', requestId };
      const requestedTimezone = accountSnapshot.exists && accountSnapshot.data().timezone
        ? accountSnapshot.data().timezone
        : 'UTC';
      const account = this.normalizeAccount(
        accountSnapshot.exists ? accountSnapshot.data() : null,
        requestedTimezone,
        nowMs
      );
      delete account.reservations[requestId];
      const alreadyInstalled = installedSnapshot.exists;
      if (completed && !request.reinstall && !alreadyInstalled) {
        const active = Object.keys(account.reservations).length;
        if (account.used + active >= this.limit) {
          transaction.set(refs.account, { ...account, updatedAtMs: nowMs }, { merge: true });
          transaction.update(refs.request, {
            status: 'failed',
            failureCode: 'RESERVATION_EXPIRED',
            settledAtMs: nowMs
          });
          return { errorCode: 'RESERVATION_EXPIRED' };
        }
        account.used += 1;
        transaction.set(refs.installed, {
          accountKey,
          appid,
          firstAddedAtMs: nowMs,
          lastAddedAtMs: nowMs
        }, { merge: true });
      } else if (completed) {
        transaction.set(refs.installed, { lastAddedAtMs: nowMs }, { merge: true });
      }
      transaction.set(refs.account, { ...account, updatedAtMs: nowMs }, { merge: true });
      transaction.update(refs.request, {
        status: completed ? 'completed' : 'failed',
        settledAtMs: nowMs
      });
      return {
        status: completed ? 'completed' : 'failed',
        requestId,
        quota: this.publicState(account, nowMs)
      };
    });
    if (result.errorCode === 'RESERVATION_EXPIRED') {
      throw new LuaShopError(
        'RESERVATION_EXPIRED',
        'The Lua add reservation expired after its slot was reassigned.',
        409
      );
    }
    return result;
  }
}

module.exports = {
  FirestoreLuaShopQuota,
  LuaShopError,
  assertAppId,
  assertRequestId,
  assertTimezone,
  localDayKey,
  nextLocalMidnightMs
};
