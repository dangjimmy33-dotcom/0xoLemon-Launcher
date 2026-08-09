const { ActivationError } = require('./errors');

class FirestoreActivationStore {
  constructor(db) {
    this.db = db;
  }

  async transaction(work) {
    return this.db.runTransaction(async (transaction) => {
      const context = {
        get: async (collection, id) => {
          const snapshot = await transaction.get(this.db.collection(collection).doc(id));
          return snapshot.exists ? snapshot.data() : null;
        },
        set: (collection, id, value, merge = false) => {
          transaction.set(this.db.collection(collection).doc(id), value, merge ? { merge: true } : undefined);
        }
      };
      return work(context);
    });
  }
}

function finiteMs(value, fallback = 0) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function activeSlots(quota, nowMs) {
  return (Array.isArray(quota?.slots) ? quota.slots : []).filter((slot) => finiteMs(slot.expiresAtMs) > nowMs);
}

function quotaSummary(config, slots, nowMs) {
  const nextAvailableAtMs = slots.length >= config.capacity
    ? Math.min(...slots.map((slot) => finiteMs(slot.expiresAtMs)))
    : null;
  return {
    capacity: config.capacity,
    available: Math.max(0, config.capacity - slots.length),
    inUse: slots.length,
    reservations: slots.filter((slot) => slot.state === 'reserved').length,
    pending: slots.filter((slot) => slot.state === 'started' || slot.state === 'uncertain').length,
    nextAvailableAt: nextAvailableAtMs ? new Date(nextAvailableAtMs).toISOString() : null,
    serverTime: new Date(nowMs).toISOString()
  };
}

class ActivationQuotaService {
  constructor(store, config, now = () => Date.now()) {
    this.store = store;
    this.config = config;
    this.now = now;
  }

  quotaId() { return this.config.gameId; }
  accountId(accountKey) { return `${this.config.gameId}_${accountKey}`; }

  async status() {
    const nowMs = this.now();
    return this.store.transaction(async (tx) => {
      const quota = await tx.get('offlineActivationQuota', this.quotaId()) || {};
      const slots = activeSlots(quota, nowMs);
      if (slots.length !== (Array.isArray(quota.slots) ? quota.slots.length : 0)) {
        tx.set('offlineActivationQuota', this.quotaId(), { slots, updatedAtMs: nowMs }, true);
      }
      return quotaSummary(this.config, slots, nowMs);
    });
  }

  async eligibility(accountKey) {
    const nowMs = this.now();
    return this.store.transaction(async (tx) => {
      const account = await tx.get('offlineActivationAccounts', this.accountId(accountKey)) || {};
      const nextEligibleAtMs = finiteMs(account.nextEligibleAtMs);
      return {
        eligible: nextEligibleAtMs <= nowMs,
        nextEligibleAt: nextEligibleAtMs > nowMs ? new Date(nextEligibleAtMs).toISOString() : null,
        serverTime: new Date(nowMs).toISOString()
      };
    });
  }

  async reserve({ requestId, accountKey, ticketHash, launcherVersion }) {
    const nowMs = this.now();
    const result = await this.store.transaction(async (tx) => {
      const request = await tx.get('offlineActivationRequests', requestId);
      const quota = await tx.get('offlineActivationQuota', this.quotaId()) || {};
      const account = await tx.get('offlineActivationAccounts', this.accountId(accountKey)) || {};
      let slots = activeSlots(quota, nowMs);

      if (request) {
        if (request.gameId !== this.config.gameId || request.accountKey !== accountKey || request.ticketHash !== ticketHash) {
          return { error: new ActivationError('IDEMPOTENCY_CONFLICT', 'This request ID was already used for different activation data.', { httpStatus: 409 }) };
        }
        if (request.status === 'success' && request.encryptedToken && finiteMs(request.tokenExpiresAtMs) > nowMs) {
          return { kind: 'replay', encryptedToken: request.encryptedToken };
        }
        if ((request.status === 'reserved' && finiteMs(request.reservationUntilMs) > nowMs) || request.status === 'started') {
          return { error: new ActivationError('REQUEST_IN_PROGRESS', 'This activation request is already in progress.', { httpStatus: 409 }) };
        }
        if (request.status === 'uncertain' && finiteMs(request.slotExpiresAtMs) > nowMs) {
          return { error: new ActivationError('OUTCOME_UNCERTAIN', 'EA may have consumed this activation. This request cannot be repeated yet.', {
            httpStatus: 409,
            retryAt: finiteMs(request.slotExpiresAtMs),
            uncertain: true
          }) };
        }
      }

      const rateWindowStartedMs = finiteMs(account.rateWindowStartedMs, nowMs);
      const rateWindowActive = nowMs - rateWindowStartedMs < this.config.accountRateWindowMs;
      const rateCount = rateWindowActive ? finiteMs(account.rateCount) : 0;
      const nextRateCount = rateCount + 1;
      const nextRateWindowStartedMs = rateWindowActive ? rateWindowStartedMs : nowMs;
      tx.set('offlineActivationAccounts', this.accountId(accountKey), {
        accountKey,
        rateWindowStartedMs: nextRateWindowStartedMs,
        rateCount: nextRateCount,
        updatedAtMs: nowMs
      }, true);

      if (nextRateCount > this.config.accountRateMax) {
        return { error: new ActivationError('ACCOUNT_RATE_LIMITED', 'Too many activation attempts from this Discord account.', {
          httpStatus: 429,
          retryAt: nextRateWindowStartedMs + this.config.accountRateWindowMs
        }) };
      }

      const nextEligibleAtMs = finiteMs(account.nextEligibleAtMs);
      if (nextEligibleAtMs > nowMs) {
        return { error: new ActivationError('ACCOUNT_COOLDOWN', 'This Discord account is still in its 96-hour activation cooldown.', {
          httpStatus: 429,
          retryAt: nextEligibleAtMs
        }) };
      }

      const accountSlot = slots.find((slot) => (
        slot.accountKey === accountKey && slot.requestId !== requestId
      ));
      if (accountSlot) {
        return { error: new ActivationError(
          'ACCOUNT_REQUEST_ACTIVE',
          'This Discord account already has an activation request using the global pool.',
          {
            httpStatus: 409,
            retryAt: finiteMs(accountSlot.expiresAtMs)
          }
        ) };
      }

      if (slots.length >= this.config.capacity) {
        const retryAt = Math.min(...slots.map((slot) => finiteMs(slot.expiresAtMs)));
        return { error: new ActivationError('NO_GLOBAL_SLOT', 'All five global activation slots are currently in use.', {
          httpStatus: 429,
          retryAt
        }) };
      }

      const reservationUntilMs = nowMs + this.config.reservationMs;
      slots = slots.filter((slot) => slot.requestId !== requestId);
      slots.push({
        requestId,
        accountKey,
        state: 'reserved',
        createdAtMs: nowMs,
        expiresAtMs: reservationUntilMs
      });
      tx.set('offlineActivationQuota', this.quotaId(), { slots, updatedAtMs: nowMs }, true);
      tx.set('offlineActivationRequests', requestId, {
        requestId,
        gameId: this.config.gameId,
        accountKey,
        ticketHash,
        launcherVersion,
        status: 'reserved',
        encryptedToken: null,
        tokenExpiresAtMs: null,
        secretExpiresAt: null,
        reservationUntilMs,
        createdAtMs: request?.createdAtMs || nowMs,
        updatedAtMs: nowMs
      }, true);
      tx.set('offlineActivationAudit', `${requestId}_reserved_${nowMs}`, {
        requestId,
        gameId: this.config.gameId,
        accountKey,
        event: 'reserved',
        createdAtMs: nowMs
      });
      return { kind: 'reserved', reservationUntilMs };
    });
    if (result.error) throw result.error;
    return result;
  }

  async markStarted(requestId, accountKey) {
    const nowMs = this.now();
    const slotExpiresAtMs = nowMs + this.config.windowMs;
    const result = await this.store.transaction(async (tx) => {
      const request = await tx.get('offlineActivationRequests', requestId);
      const quota = await tx.get('offlineActivationQuota', this.quotaId()) || {};
      if (!request || request.accountKey !== accountKey || request.status !== 'reserved') {
        return { error: new ActivationError('REQUEST_STATE_INVALID', 'The activation reservation is no longer valid.', { httpStatus: 409 }) };
      }
      const slots = activeSlots(quota, nowMs);
      const slot = slots.find((candidate) => candidate.requestId === requestId);
      if (!slot) {
        return { error: new ActivationError('REQUEST_STATE_INVALID', 'The activation reservation expired before EA was contacted.', { httpStatus: 409 }) };
      }
      slot.state = 'started';
      slot.expiresAtMs = slotExpiresAtMs;
      tx.set('offlineActivationQuota', this.quotaId(), { slots, updatedAtMs: nowMs }, true);
      tx.set('offlineActivationRequests', requestId, {
        status: 'started',
        eaStartedAtMs: nowMs,
        slotExpiresAtMs,
        updatedAtMs: nowMs
      }, true);
      return { slotExpiresAtMs };
    });
    if (result.error) throw result.error;
    return result;
  }

  async completeSuccess(requestId, accountKey, encryptedToken) {
    const nowMs = this.now();
    const slotExpiresAtMs = nowMs + this.config.windowMs;
    const nextEligibleAtMs = nowMs + this.config.cooldownMs;
    const result = await this.store.transaction(async (tx) => {
      const request = await tx.get('offlineActivationRequests', requestId);
      const quota = await tx.get('offlineActivationQuota', this.quotaId()) || {};
      const account = await tx.get('offlineActivationAccounts', this.accountId(accountKey)) || {};
      if (!request || request.accountKey !== accountKey || !['started', 'success'].includes(request.status)) {
        return { error: new ActivationError('OUTCOME_UNCERTAIN', 'The token was issued but its activation record could not be finalized.', {
          httpStatus: 503,
          uncertain: true
        }) };
      }

      const slots = activeSlots(quota, nowMs).filter((slot) => slot.requestId !== requestId);
      slots.push({ requestId, accountKey, state: 'success', createdAtMs: nowMs, expiresAtMs: slotExpiresAtMs });
      tx.set('offlineActivationQuota', this.quotaId(), { slots, updatedAtMs: nowMs }, true);
      tx.set('offlineActivationAccounts', this.accountId(accountKey), {
        ...account,
        accountKey,
        lastSuccessAtMs: nowMs,
        nextEligibleAtMs,
        updatedAtMs: nowMs
      });
      tx.set('offlineActivationRequests', requestId, {
        status: 'success',
        encryptedToken,
        tokenExpiresAtMs: nextEligibleAtMs,
        // Configure Firestore TTL on this field so encrypted recovery tokens are
        // deleted after the idempotency/cooldown window.
        secretExpiresAt: new Date(nextEligibleAtMs),
        completedAtMs: nowMs,
        slotExpiresAtMs,
        updatedAtMs: nowMs
      }, true);
      tx.set('offlineActivationAudit', `${requestId}_success_${nowMs}`, {
        requestId,
        gameId: this.config.gameId,
        accountKey,
        event: 'success',
        createdAtMs: nowMs
      });
      return { nextEligibleAtMs, slotExpiresAtMs };
    });
    if (result.error) throw result.error;
    return result;
  }

  async completeFailure(requestId, accountKey, uncertain, code) {
    const nowMs = this.now();
    const result = await this.store.transaction(async (tx) => {
      const request = await tx.get('offlineActivationRequests', requestId);
      const quota = await tx.get('offlineActivationQuota', this.quotaId()) || {};
      if (!request || request.accountKey !== accountKey) return { ignored: true };
      // Once the token and cooldown have committed, a later transport/status error
      // must never downgrade the idempotent recovery record.
      if (request.status === 'success') return { ignored: true, completed: true };
      let slots = activeSlots(quota, nowMs);
      const slot = slots.find((candidate) => candidate.requestId === requestId);
      let slotExpiresAtMs = finiteMs(request.slotExpiresAtMs, nowMs + this.config.windowMs);
      if (uncertain) {
        if (slot) {
          slot.state = 'uncertain';
          slot.expiresAtMs = Math.max(finiteMs(slot.expiresAtMs), slotExpiresAtMs);
          slotExpiresAtMs = slot.expiresAtMs;
        } else {
          slots.push({ requestId, accountKey, state: 'uncertain', createdAtMs: nowMs, expiresAtMs: slotExpiresAtMs });
        }
      } else {
        slots = slots.filter((candidate) => candidate.requestId !== requestId);
      }
      tx.set('offlineActivationQuota', this.quotaId(), { slots, updatedAtMs: nowMs }, true);
      tx.set('offlineActivationRequests', requestId, {
        status: uncertain ? 'uncertain' : 'failed',
        failureCode: code,
        slotExpiresAtMs: uncertain ? slotExpiresAtMs : null,
        updatedAtMs: nowMs
      }, true);
      tx.set('offlineActivationAudit', `${requestId}_${uncertain ? 'uncertain' : 'failed'}_${nowMs}`, {
        requestId,
        gameId: this.config.gameId,
        accountKey,
        event: uncertain ? 'uncertain' : 'failed',
        code,
        createdAtMs: nowMs
      });
      return { ignored: false, slotExpiresAtMs: uncertain ? slotExpiresAtMs : null };
    });
    return result;
  }

  async cancel(requestId, accountKey) {
    const nowMs = this.now();
    const result = await this.store.transaction(async (tx) => {
      const request = await tx.get('offlineActivationRequests', requestId);
      const quota = await tx.get('offlineActivationQuota', this.quotaId()) || {};
      if (!request || request.accountKey !== accountKey || request.status !== 'reserved') {
        return { error: new ActivationError('CANCEL_NOT_ALLOWED', 'Activation can only be canceled before EA is contacted.', { httpStatus: 409 }) };
      }
      const slots = activeSlots(quota, nowMs).filter((slot) => slot.requestId !== requestId);
      tx.set('offlineActivationQuota', this.quotaId(), { slots, updatedAtMs: nowMs }, true);
      tx.set('offlineActivationRequests', requestId, { status: 'canceled', updatedAtMs: nowMs }, true);
      return { canceled: true };
    });
    if (result.error) throw result.error;
    return result;
  }
}

module.exports = {
  ActivationQuotaService,
  FirestoreActivationStore,
  activeSlots,
  quotaSummary
};
