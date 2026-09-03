const crypto = require('crypto');

const { SocialError } = require('./errors');

const USERNAME_PATTERN = /^[a-z0-9._-]{1,80}$/i;
const DISCORD_ID_PATTERN = /^\d{15,22}$/;
const REQUEST_ID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function normalizeSearch(value) {
  return String(value || '')
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9._ -]/gi, ' ')
    .toLowerCase()
    .trim()
    .replace(/\s+/g, ' ')
    .slice(0, 80);
}

function searchPrefixes(...values) {
  const result = new Set();
  for (const value of values) {
    const normalized = normalizeSearch(value);
    for (let length = 1; length <= Math.min(normalized.length, 40); length += 1) {
      result.add(normalized.slice(0, length));
    }
  }
  return [...result].slice(0, 100);
}

function boundedString(value, max, fallback = '') {
  return typeof value === 'string'
    ? value.replace(/[\u0000-\u001f\u007f]/g, ' ').trim().slice(0, max)
    : fallback;
}

function pairMembers(first, second) {
  if (!DISCORD_ID_PATTERN.test(String(first)) || !DISCORD_ID_PATTERN.test(String(second))) {
    throw new SocialError('INVALID_USER', 'A valid Discord user is required.');
  }
  if (first === second) throw new SocialError('SELF_RELATIONSHIP', 'You cannot perform this action on yourself.');
  return [String(first), String(second)].sort();
}

function dayKeyUtc(ms) {
  return new Date(ms).toISOString().slice(0, 10);
}

function relationshipFor(document, userId) {
  if (!document) return 'none';
  if (Array.isArray(document.blockedBy) && document.blockedBy.length) return 'blocked';
  if (document.status === 'accepted') return 'friend';
  if (document.status === 'pending') return document.requestedBy === userId ? 'pending-out' : 'pending-in';
  return 'none';
}

const PRESENCE_PRIORITY = Object.freeze({ offline: 0, online: 1, idle: 2, playing: 3 });

function activePresenceSessions(document, nowMs) {
  const source = Array.isArray(document && document.sessions)
    ? document.sessions
    : document && document.sessionId ? [document] : [];
  return source
    .filter((session) => session && typeof session.sessionId === 'string' && Number(session.expiresAtMs) > nowMs)
    .sort((left, right) => {
      const priority = (PRESENCE_PRIORITY[right.state] || 0) - (PRESENCE_PRIORITY[left.state] || 0);
      return priority || (Number(right.lastSeenAtMs) || 0) - (Number(left.lastSeenAtMs) || 0);
    })
    .slice(0, 8);
}

function aggregatePresence(sessions, nowMs) {
  const active = activePresenceSessions({ sessions }, nowMs);
  const selected = active[0];
  if (!selected) {
    return {
      sessions: [],
      sessionId: '',
      state: 'offline',
      activityKind: 'none',
      activityLabel: 'Offline',
      activityDetail: '',
      gameId: '',
      sinceMs: nowMs,
      lastSeenAtMs: nowMs,
      expiresAtMs: nowMs
    };
  }
  return { ...selected, sessions: active };
}

class SocialService {
  constructor(db, config, eventHub, { now = () => Date.now() } = {}) {
    this.db = db;
    this.config = config;
    this.eventHub = eventHub;
    this.now = now;
  }

  accountKey(discordId) {
    if (this.config.accountHmacKey.length < 32) {
      throw new SocialError('SOCIAL_NOT_CONFIGURED', 'Social account protection is not configured.', 503);
    }
    return crypto.createHmac('sha256', this.config.accountHmacKey).update(String(discordId)).digest('hex');
  }

  pairKey(first, second) {
    const members = pairMembers(first, second);
    return crypto.createHmac('sha256', this.config.accountHmacKey).update(members.join(':')).digest('hex');
  }

  assertEnabled(identity) {
    if (!this.config.enabled) throw new SocialError('SOCIAL_DISABLED', 'Social services are not enabled.', 503);
    if (this.config.canaryMode && !this.config.canaryDiscordIds.has(identity.id)) {
      throw new SocialError('SOCIAL_CANARY_ONLY', 'Social services are currently available to test accounts only.', 403);
    }
  }

  async ensureProfile(identity) {
    this.assertEnabled(identity);
    const ref = this.db.collection('socialUsers').doc(identity.id);
    const snapshot = await ref.get();
    const current = snapshot.exists ? snapshot.data() : {};
    const nowMs = this.now();
    const profile = {
      schemaVersion: 1,
      discordId: identity.id,
      username: boundedString(identity.username, 80, `user-${identity.id.slice(-6)}`),
      displayName: boundedString(identity.displayName, 80, identity.username),
      avatarUrl: boundedString(identity.avatarUrl, 512),
      accountCreatedAt: boundedString(identity.accountCreatedAt, 40),
      bio: boundedString(current.bio, 480),
      customStatus: boundedString(current.customStatus, 120),
      accent: boundedString(current.accent, 32, '42 88% 58%'),
      coverHash: boundedString(current.coverHash, 64),
      coverPath: boundedString(current.coverPath, 512),
      coverRevision: Number.isSafeInteger(current.coverRevision) ? current.coverRevision : 0,
      coverUpdatedAtMs: Number(current.coverUpdatedAtMs) || 0,
      coverPosition: Math.max(0, Math.min(100, Number(current.coverPosition) || 50)),
      appearOffline: current.appearOffline === true,
      privacy: current.privacy && typeof current.privacy === 'object'
        ? current.privacy
        : { profile: 'members', presence: 'members' },
      leaderboardOptIn: current.leaderboardOptIn === true,
      searchPrefixes: searchPrefixes(identity.username, identity.displayName),
      createdAtMs: Number(current.createdAtMs) || nowMs,
      updatedAtMs: nowMs
    };
    await ref.set(profile, { merge: true });
    return profile;
  }

  publicProfile(profile, presence = null, relationship = 'none', stats = null) {
    const nowMs = this.now();
    const present = presence && Number(presence.expiresAtMs) > nowMs && !profile.appearOffline;
    const state = present ? boundedString(presence.state, 16, 'online') : 'offline';
    return {
      id: profile.discordId,
      username: profile.username,
      displayName: profile.displayName,
      avatarUrl: profile.avatarUrl || '',
      bio: profile.bio || '',
      customStatus: profile.customStatus || '',
      accent: profile.accent || '42 88% 58%',
      coverUrl: profile.coverPath
        ? `https://huggingface.co/datasets/${this.config.cover.repoName}/resolve/${encodeURIComponent(this.config.cover.branch)}/${profile.coverPath}`
        : null,
      coverHash: profile.coverHash || null,
      coverRevision: Number(profile.coverRevision) || 0,
      coverPosition: Number(profile.coverPosition) || 50,
      presence: profile.appearOffline ? 'offline' : state,
      activity: present ? {
        kind: boundedString(presence.activityKind, 16, 'launcher'),
        label: boundedString(presence.activityLabel, 120, 'Using 0xoLemon'),
        detail: boundedString(presence.activityDetail, 160),
        gameId: boundedString(presence.gameId, 100) || null,
        sinceMs: Number(presence.sinceMs) || Number(presence.lastSeenAtMs) || nowMs
      } : { kind: 'none', label: 'Offline', detail: '', gameId: null, sinceMs: 0 },
      relationship,
      appearOffline: relationship === 'self' ? profile.appearOffline === true : undefined,
      leaderboardOptIn: profile.leaderboardOptIn === true,
      stats: stats || { downloads: 0, downloadedBytes: 0, playMinutes: 0, onlineMinutes: 0, gamesPlayed: 0 },
      joinedAt: new Date(Number(profile.createdAtMs) || nowMs).toISOString()
    };
  }

  async relationshipDocuments(userId) {
    const snapshot = await this.db.collection('socialRelationships').where('members', 'array-contains', userId).get();
    return snapshot.docs.map((doc) => ({ id: doc.id, ...doc.data() }));
  }

  async loadProfiles(userIds) {
    const unique = [...new Set(userIds)].filter(Boolean);
    const values = await Promise.all(unique.map(async (id) => {
      const [profile, presence, totals] = await Promise.all([
        this.db.collection('socialUsers').doc(id).get(),
        this.db.collection('socialPresence').doc(id).get(),
        this.db.collection('socialStatsTotals').doc(id).get()
      ]);
      return profile.exists ? [id, {
        profile: profile.data(),
        presence: presence.exists ? presence.data() : null,
        stats: totals.exists ? totals.data() : null
      }] : null;
    }));
    return new Map(values.filter(Boolean));
  }

  async bootstrap(identity) {
    const selfProfile = await this.ensureProfile(identity);
    const relationships = await this.relationshipDocuments(identity.id);
    const relatedIds = relationships.flatMap((entry) => entry.members || []).filter((id) => id !== identity.id);
    const profiles = await this.loadProfiles([identity.id, ...relatedIds]);
    const selfData = profiles.get(identity.id) || { profile: selfProfile, presence: null, stats: null };
    const members = [];
    for (const relationship of relationships) {
      const otherId = (relationship.members || []).find((id) => id !== identity.id);
      const data = profiles.get(otherId);
      if (!data) continue;
      members.push(this.publicProfile(data.profile, data.presence, relationshipFor(relationship, identity.id), data.stats));
    }
    return {
      serverTime: new Date(this.now()).toISOString(),
      presenceHeartbeatMs: this.config.presence.heartbeatMs,
      presenceStaleMs: this.config.presence.staleMs,
      self: this.publicProfile(selfData.profile, selfData.presence, 'self', selfData.stats),
      members,
      canary: this.config.canaryMode,
      coverBatchMs: this.config.cover.batchMs
    };
  }

  async search(identity, query, limit = 24, cursor = null) {
    await this.ensureProfile(identity);
    const normalized = normalizeSearch(query);
    if (!normalized) return { items: [], nextCursor: null };
    const max = Math.max(1, Math.min(40, Number(limit) || 24));
    let snapshots;
    if (DISCORD_ID_PATTERN.test(normalized)) {
      const exact = await this.db.collection('socialUsers').doc(normalized).get();
      snapshots = exact.exists ? [exact] : [];
    } else {
      const users = this.db.collection('socialUsers');
      let searchQuery = users.where('searchPrefixes', 'array-contains', normalized.slice(0, 40));
      if (cursor) {
        if (!DISCORD_ID_PATTERN.test(String(cursor))) {
          throw new SocialError('INVALID_CURSOR', 'The social search cursor is invalid.');
        }
        const cursorSnapshot = await users.doc(String(cursor)).get();
        if (!cursorSnapshot.exists) throw new SocialError('INVALID_CURSOR', 'The social search cursor has expired.');
        searchQuery = searchQuery.startAfter(cursorSnapshot);
      }
      snapshots = (await searchQuery.limit(max + 1).get()).docs;
    }
    const relationDocs = await this.relationshipDocuments(identity.id);
    const relationByUser = new Map();
    for (const relation of relationDocs) {
      const other = relation.members.find((id) => id !== identity.id);
      relationByUser.set(other, relationshipFor(relation, identity.id));
    }
    const page = snapshots.slice(0, max);
    const ids = page.map((item) => item.id).filter((id) => id !== identity.id);
    const profiles = await this.loadProfiles(ids);
    return {
      items: ids.map((id) => {
        const data = profiles.get(id);
        return this.publicProfile(data.profile, data.presence, relationByUser.get(id) || 'none', data.stats);
      }),
      nextCursor: snapshots.length > max ? page.at(-1)?.id || null : null
    };
  }

  async getProfile(identity, targetId) {
    await this.ensureProfile(identity);
    if (!DISCORD_ID_PATTERN.test(String(targetId || ''))) {
      throw new SocialError('INVALID_USER', 'A valid Discord user is required.');
    }
    const [profiles, relationships] = await Promise.all([
      this.loadProfiles([targetId]),
      this.relationshipDocuments(identity.id)
    ]);
    const data = profiles.get(String(targetId));
    if (!data) throw new SocialError('SOCIAL_USER_NOT_FOUND', 'The social profile was not found.', 404);
    const relationship = relationships.find((entry) => Array.isArray(entry.members) && entry.members.includes(String(targetId)));
    return this.publicProfile(
      data.profile,
      data.presence,
      targetId === identity.id ? 'self' : relationshipFor(relationship, identity.id),
      data.stats
    );
  }

  async updateProfile(identity, input) {
    const current = await this.ensureProfile(identity);
    const patch = {
      bio: boundedString(input.bio, 480, current.bio),
      customStatus: boundedString(input.customStatus, 120, current.customStatus),
      accent: /^\d{1,3} \d{1,3}% \d{1,3}%$/.test(String(input.accent || '')) ? input.accent : current.accent,
      coverPosition: Math.max(0, Math.min(100, Number(input.coverPosition ?? current.coverPosition))),
      appearOffline: typeof input.appearOffline === 'boolean'
        ? input.appearOffline
        : current.appearOffline === true,
      updatedAtMs: this.now()
    };
    await this.db.collection('socialUsers').doc(identity.id).set(patch, { merge: true });
    const profile = { ...current, ...patch };
    const [presenceSnapshot, statsSnapshot] = await Promise.all([
      this.db.collection('socialPresence').doc(identity.id).get(),
      this.db.collection('socialStatsTotals').doc(identity.id).get()
    ]);
    const view = this.publicProfile(
      profile,
      presenceSnapshot.exists ? presenceSnapshot.data() : null,
      'self',
      statsSnapshot.exists ? statsSnapshot.data() : null
    );
    this.eventHub.publish(identity.tenantId, [], 'profile.updated', { profile: view });
    return view;
  }

  async mutateRelationship(identity, targetId, action, requestId = '') {
    await this.ensureProfile(identity);
    const normalizedRequestId = String(requestId || '');
    if (normalizedRequestId && !REQUEST_ID_PATTERN.test(normalizedRequestId)) {
      throw new SocialError('INVALID_REQUEST_ID', 'A UUID request ID is required.');
    }
    const members = pairMembers(identity.id, targetId);
    const targetSnapshot = await this.db.collection('socialUsers').doc(String(targetId)).get();
    if (!targetSnapshot.exists) throw new SocialError('SOCIAL_USER_NOT_FOUND', 'The social profile was not found.', 404);
    const pairKey = this.pairKey(identity.id, targetId);
    const ref = this.db.collection('socialRelationships').doc(pairKey);
    const operationRef = normalizedRequestId
      ? this.db.collection('socialRelationshipOperations').doc(
        crypto.createHmac('sha256', this.config.accountHmacKey)
          .update(`${identity.id}:${normalizedRequestId}`)
          .digest('hex')
      )
      : null;
    const nowMs = this.now();
    const outcome = await this.db.runTransaction(async (transaction) => {
      if (operationRef) {
        const operationSnapshot = await transaction.get(operationRef);
        if (operationSnapshot.exists) {
          const stored = operationSnapshot.data();
          if (stored.targetId !== String(targetId) || stored.action !== action) {
            throw new SocialError('REQUEST_ID_REUSED', 'This request ID was already used for another social action.', 409);
          }
          return { payload: stored.payload, replayed: true };
        }
      }
      const snapshot = await transaction.get(ref);
      const current = snapshot.exists ? snapshot.data() : null;
      const blockedBy = new Set(Array.isArray(current && current.blockedBy) ? current.blockedBy : []);
      let result;
      let changed = false;
      if (action === 'block') {
        if (current && current.status === 'blocked' && blockedBy.has(identity.id)) {
          result = current;
        } else {
          blockedBy.add(identity.id);
          const next = { members, status: 'blocked', blockedBy: [...blockedBy], requestedBy: null, updatedAtMs: nowMs };
          transaction.set(ref, next);
          result = next;
          changed = true;
        }
      } else if (action === 'unblock') {
        if (!current || !blockedBy.has(identity.id)) {
          result = current;
        } else {
          blockedBy.delete(identity.id);
          if (!blockedBy.size) {
            transaction.delete(ref);
            result = null;
          } else {
            const next = { ...current, status: 'blocked', blockedBy: [...blockedBy], updatedAtMs: nowMs };
            transaction.set(ref, next);
            result = next;
          }
          changed = true;
        }
      } else {
        if (blockedBy.size) throw new SocialError('RELATIONSHIP_BLOCKED', 'This relationship is blocked.', 409);
        if (action === 'request') {
          if (current && current.status === 'accepted') {
            result = current;
          } else {
            const reciprocal = current && current.status === 'pending' && current.requestedBy !== identity.id;
            const next = {
              members,
              status: reciprocal ? 'accepted' : 'pending',
              requestedBy: reciprocal ? null : identity.id,
              blockedBy: [],
              createdAtMs: Number(current && current.createdAtMs) || nowMs,
              updatedAtMs: nowMs
            };
            transaction.set(ref, next);
            result = next;
            changed = true;
          }
        } else if (action === 'accept') {
          if (current && current.status === 'accepted') {
            result = current;
          } else {
            if (!current || current.status !== 'pending' || current.requestedBy === identity.id) {
              throw new SocialError('FRIEND_REQUEST_NOT_FOUND', 'No incoming friend request was found.', 404);
            }
            const next = { ...current, status: 'accepted', requestedBy: null, updatedAtMs: nowMs };
            transaction.set(ref, next);
            result = next;
            changed = true;
          }
        } else if (action === 'cancel' || action === 'decline') {
          if (!current) {
            result = null;
          } else {
            const ownsRequest = current.status === 'pending' && current.requestedBy === identity.id;
            const receivedRequest = current.status === 'pending' && current.requestedBy !== identity.id;
            if ((action === 'cancel' && !ownsRequest) || (action === 'decline' && !receivedRequest)) {
              throw new SocialError('FRIEND_REQUEST_NOT_FOUND', 'No matching friend request was found.', 404);
            }
            transaction.delete(ref);
            result = null;
            changed = true;
          }
        } else if (action === 'remove') {
          if (!current) {
            result = null;
          } else {
            if (current.status !== 'accepted') {
              throw new SocialError('FRIENDSHIP_NOT_FOUND', 'No friendship was found.', 404);
            }
            transaction.delete(ref);
            result = null;
            changed = true;
          }
        } else {
          throw new SocialError('INVALID_RELATIONSHIP_ACTION', 'Unknown relationship action.');
        }
      }

      const payload = {
        userId: targetId,
        relationship: relationshipFor(result, identity.id),
        counterpartRelationship: relationshipFor(result, targetId),
        requestId: normalizedRequestId || null
      };
      if (operationRef) {
        transaction.create(operationRef, {
          userId: identity.id,
          targetId: String(targetId),
          action,
          payload,
          createdAtMs: nowMs,
          expiresAtMs: nowMs + 7 * 24 * 60 * 60 * 1000
        });
      }
      return { payload, replayed: false, changed };
    });
    if (!outcome.replayed && outcome.changed) {
      this.eventHub.publish(identity.tenantId, members, 'relationship.updated', outcome.payload);
    }
    return outcome.payload;
  }

  async heartbeat(identity, input) {
    const profile = await this.ensureProfile(identity);
    const nowMs = this.now();
    const allowedStates = new Set(['online', 'idle', 'playing', 'offline']);
    const requestedState = allowedStates.has(input.state) ? input.state : 'online';
    const sessionId = boundedString(input.sessionId, 80);
    if (!sessionId) throw new SocialError('INVALID_SESSION', 'A social presence session ID is required.');
    const ref = this.db.collection('socialPresence').doc(identity.id);
    const presence = await this.db.runTransaction(async (transaction) => {
      const snapshot = await transaction.get(ref);
      const current = snapshot.exists ? snapshot.data() : {};
      const sessions = activePresenceSessions(current, nowMs)
        .filter((session) => session.sessionId !== sessionId);
      if (requestedState !== 'offline') {
        sessions.push({
          sessionId,
          state: requestedState,
          activityKind: requestedState === 'playing' ? 'game' : 'launcher',
          activityLabel: boundedString(input.activityLabel, 120, requestedState === 'playing' ? 'Playing' : 'Using 0xoLemon'),
          activityDetail: boundedString(input.activityDetail, 160),
          gameId: boundedString(input.gameId, 100),
          sinceMs: Number(input.sinceMs) > 0 ? Math.min(Number(input.sinceMs), nowMs) : nowMs,
          lastSeenAtMs: nowMs,
          expiresAtMs: nowMs + this.config.presence.staleMs
        });
      }
      const aggregate = {
        schemaVersion: 2,
        discordId: identity.id,
        ...aggregatePresence(sessions, nowMs)
      };
      transaction.set(ref, aggregate);
      return aggregate;
    });
    const view = this.publicProfile(profile, presence, 'self', null);
    this.eventHub.publish(identity.tenantId, [], 'presence.updated', { userId: identity.id, presence: view.presence, activity: view.activity });
    return { serverTime: new Date(nowMs).toISOString(), expiresAt: new Date(presence.expiresAtMs).toISOString() };
  }

  async setLeaderboardParticipation(identity, optIn) {
    const profile = await this.ensureProfile(identity);
    await this.db.collection('socialUsers').doc(identity.id).set({ leaderboardOptIn: optIn === true, updatedAtMs: this.now() }, { merge: true });
    return { leaderboardOptIn: optIn === true, profileRevision: Number(profile.coverRevision) || 0 };
  }

  async recordStatEvent(identity, input) {
    await this.ensureProfile(identity);
    if (!REQUEST_ID_PATTERN.test(String(input.eventId || ''))) throw new SocialError('INVALID_EVENT_ID', 'A UUID eventId is required.');
    const allowedKinds = new Set(['heartbeat', 'game-session', 'install']);
    if (!allowedKinds.has(input.kind)) throw new SocialError('INVALID_STAT_EVENT', 'Unsupported statistic event.');
    const delta = {
      downloads: input.kind === 'install' ? Math.min(1, Math.max(0, Number(input.downloads) || 0)) : 0,
      downloadedBytes: input.kind === 'install' ? Math.min(2 * 1024 ** 4, Math.max(0, Number(input.downloadedBytes) || 0)) : 0,
      playMinutes: input.kind === 'game-session' ? Math.min(12 * 60, Math.max(0, Number(input.playMinutes) || 0)) : 0,
      onlineMinutes: Math.min(15, Math.max(0, Number(input.onlineMinutes) || 0)),
      gamesPlayed: input.kind === 'game-session' ? Math.min(1, Math.max(0, Number(input.gamesPlayed) || 0)) : 0
    };
    const nowMs = this.now();
    const eventKey = crypto.createHmac('sha256', this.config.accountHmacKey)
      .update(`${identity.id}:${input.eventId}`)
      .digest('hex');
    const eventRef = this.db.collection('socialStatEvents').doc(eventKey);
    const totalRef = this.db.collection('socialStatsTotals').doc(identity.id);
    const dayRef = this.db.collection('socialStatsDaily').doc(`${identity.id}_${dayKeyUtc(nowMs)}`);
    const applied = await this.db.runTransaction(async (transaction) => {
      const eventSnapshot = await transaction.get(eventRef);
      if (eventSnapshot.exists) return false;
      const [totalSnapshot, daySnapshot] = await Promise.all([transaction.get(totalRef), transaction.get(dayRef)]);
      const add = (current) => ({
        discordId: identity.id,
        dayKey: dayKeyUtc(nowMs),
        downloads: (Number(current.downloads) || 0) + delta.downloads,
        downloadedBytes: (Number(current.downloadedBytes) || 0) + delta.downloadedBytes,
        playMinutes: (Number(current.playMinutes) || 0) + delta.playMinutes,
        onlineMinutes: (Number(current.onlineMinutes) || 0) + delta.onlineMinutes,
        gamesPlayed: (Number(current.gamesPlayed) || 0) + delta.gamesPlayed,
        updatedAtMs: nowMs
      });
      transaction.create(eventRef, { discordId: identity.id, eventId: input.eventId, kind: input.kind, createdAtMs: nowMs });
      transaction.set(totalRef, add(totalSnapshot.exists ? totalSnapshot.data() : {}));
      transaction.set(dayRef, add(daySnapshot.exists ? daySnapshot.data() : {}));
      return true;
    });
    return { applied };
  }

  async leaderboard(identity, { metric = 'playMinutes', period = 'all', scope = 'global', limit = 50 } = {}) {
    await this.ensureProfile(identity);
    const allowedMetrics = new Set(['downloads', 'playMinutes', 'onlineMinutes']);
    const selectedMetric = allowedMetrics.has(metric) ? metric : 'playMinutes';
    const max = Math.max(1, Math.min(100, Number(limit) || 50));
    const profilesSnapshot = await this.db.collection('socialUsers').where('leaderboardOptIn', '==', true).limit(500).get();
    let allowedIds = new Set(profilesSnapshot.docs.map((doc) => doc.id));
    if (scope === 'friends') {
      const relations = await this.relationshipDocuments(identity.id);
      const friends = new Set(relations.filter((entry) => entry.status === 'accepted').flatMap((entry) => entry.members));
      allowedIds = new Set([...allowedIds].filter((id) => friends.has(id)));
    }
    let statDocuments;
    if (period === 'all') {
      statDocuments = (await this.db.collection('socialStatsTotals').limit(1000).get()).docs.map((doc) => doc.data());
    } else {
      const days = period === 'week' ? 7 : 31;
      const start = dayKeyUtc(this.now() - (days - 1) * 24 * 60 * 60 * 1000);
      statDocuments = (await this.db.collection('socialStatsDaily').where('dayKey', '>=', start).limit(5000).get()).docs.map((doc) => doc.data());
    }
    const totals = new Map();
    for (const stats of statDocuments) {
      if (!allowedIds.has(stats.discordId)) continue;
      const current = totals.get(stats.discordId) || { downloads: 0, downloadedBytes: 0, playMinutes: 0, onlineMinutes: 0, gamesPlayed: 0 };
      for (const key of Object.keys(current)) current[key] += Number(stats[key]) || 0;
      totals.set(stats.discordId, current);
    }
    const profiles = await this.loadProfiles([...totals.keys()]);
    const entries = [...totals.entries()]
      .sort((a, b) => (b[1][selectedMetric] - a[1][selectedMetric]) || a[0].localeCompare(b[0]))
      .slice(0, max)
      .map(([id, stats], index) => {
        const data = profiles.get(id);
        return { rank: index + 1, profile: this.publicProfile(data.profile, data.presence, id === identity.id ? 'self' : 'none', stats), score: stats[selectedMetric] };
      });
    return { metric: selectedMetric, period, scope, entries, nextCursor: null, communityDataNotice: true };
  }
}

module.exports = {
  SocialService,
  dayKeyUtc,
  normalizeSearch,
  relationshipFor,
  searchPrefixes
};
