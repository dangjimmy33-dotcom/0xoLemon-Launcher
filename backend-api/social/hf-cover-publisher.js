const crypto = require('crypto');

const { SocialError } = require('./errors');

function conflictLike(error) {
  const status = Number(error && (error.statusCode || error.status));
  return status === 409 || /409|conflict|parent commit/i.test(String(error && error.message || ''));
}

async function validateCoverBuffer(buffer, maxBytes) {
  if (!Buffer.isBuffer(buffer) || !buffer.length) throw new SocialError('COVER_REQUIRED', 'A WebP profile cover is required.');
  if (buffer.length > maxBytes) throw new SocialError('COVER_TOO_LARGE', `Profile cover exceeds ${maxBytes} bytes.`, 413);
  if (buffer.length < 16 || buffer.subarray(0, 4).toString('ascii') !== 'RIFF' || buffer.subarray(8, 12).toString('ascii') !== 'WEBP') {
    throw new SocialError('INVALID_COVER', 'Profile cover must be a valid WebP image.');
  }
  const sharp = require('sharp');
  let metadata;
  try {
    metadata = await sharp(buffer, { failOn: 'warning', limitInputPixels: 1280 * 720 * 2 }).metadata();
  } catch {
    throw new SocialError('INVALID_COVER', 'Profile cover could not be decoded.');
  }
  if (metadata.format !== 'webp' || metadata.width !== 1280 || metadata.height !== 360) {
    throw new SocialError('INVALID_COVER_DIMENSIONS', 'Profile cover must be a 1280x360 WebP image.');
  }
  return crypto.createHash('sha256').update(buffer).digest('hex');
}

async function parentCommit(config) {
  const [namespace, repo] = config.repoName.split('/');
  if (!namespace || !repo) throw new Error('HF_REPO_NAME_INVALID');
  const response = await fetch(
    `https://huggingface.co/api/datasets/${encodeURIComponent(namespace)}/${encodeURIComponent(repo)}/revision/${encodeURIComponent(config.branch)}`,
    { headers: { Authorization: `Bearer ${config.token}` } }
  );
  if (!response.ok) throw new Error(`HF_REPO_INFO_${response.status}`);
  const payload = await response.json();
  if (typeof payload.sha !== 'string' || !/^[a-f0-9]{40,64}$/i.test(payload.sha)) throw new Error('HF_REPO_INFO_INVALID');
  return payload.sha;
}

async function verifyObject(config, path) {
  const response = await fetch(
    `https://huggingface.co/datasets/${config.repoName}/resolve/${encodeURIComponent(config.branch)}/${path}`,
    { method: 'HEAD', headers: { Authorization: `Bearer ${config.token}`, 'Cache-Control': 'no-cache' }, redirect: 'follow' }
  );
  if (!response.ok) throw new Error(`HF_COVER_VERIFY_${response.status}`);
}

class HfCoverPublisher {
  constructor({ config, getTenantDb, eventHub, now = () => Date.now(), autoStart = true }) {
    this.config = config;
    this.getTenantDb = getTenantDb;
    this.eventHub = eventHub;
    this.now = now;
    this.pending = new Map();
    this.hydratedTenants = new Set();
    this.flushing = false;
    this.timer = null;
    if (autoStart) {
      this.timer = setInterval(() => void this.flush(), config.batchMs);
      if (typeof this.timer.unref === 'function') this.timer.unref();
    }
  }

  configured() {
    return Boolean(this.config.token && this.config.repoName);
  }

  queueKey(tenantId, userId) {
    return `${tenantId}:${userId}`;
  }

  async enqueue({ tenantId, userId, accountKey, buffer, expectedHash }) {
    if (!this.configured()) throw new SocialError('COVER_PUBLISH_NOT_CONFIGURED', 'Profile cover publishing is not configured.', 503);
    const hash = await validateCoverBuffer(buffer, this.config.maxBytes);
    if (expectedHash && expectedHash !== hash) throw new SocialError('COVER_HASH_MISMATCH', 'Profile cover hash does not match its contents.', 409);
    const path = `covers/${accountKey.slice(0, 2)}/${accountKey}/${hash}.webp`;
    const item = {
      tenantId,
      userId,
      accountKey,
      hash,
      path,
      buffer: Buffer.from(buffer),
      queuedAtMs: this.now()
    };
    await this.getTenantDb(tenantId).collection('socialCoverQueue').doc(accountKey).set({
      schemaVersion: 1,
      userId,
      accountKey,
      hash,
      path,
      contentBase64: item.buffer.toString('base64'),
      queuedAtMs: item.queuedAtMs
    });
    this.pending.set(this.queueKey(tenantId, userId), item);
    return {
      status: 'queued',
      hash,
      queuedAt: new Date(this.now()).toISOString(),
      publishBy: new Date(this.now() + this.config.batchMs).toISOString()
    };
  }

  async hydrateTenant(tenantId) {
    if (this.hydratedTenants.has(tenantId)) return;
    this.hydratedTenants.add(tenantId);
    try {
      const snapshot = await this.getTenantDb(tenantId).collection('socialCoverQueue').limit(500).get();
      for (const document of snapshot.docs) {
        const data = document.data();
        const buffer = Buffer.from(String(data.contentBase64 || ''), 'base64');
        if (!data.userId || !data.accountKey || !data.hash || !data.path || !buffer.length) continue;
        this.pending.set(this.queueKey(tenantId, data.userId), {
          tenantId,
          userId: data.userId,
          accountKey: data.accountKey,
          hash: data.hash,
          path: data.path,
          buffer,
          queuedAtMs: Number(data.queuedAtMs) || this.now()
        });
      }
    } catch (error) {
      this.hydratedTenants.delete(tenantId);
      throw error;
    }
  }

  async commitOperations(operations, title) {
    const hub = require('@huggingface/hub');
    const repo = { type: 'dataset', name: this.config.repoName };
    let lastError;
    for (let attempt = 0; attempt < 3; attempt += 1) {
      try {
        const parent = await parentCommit(this.config);
        await hub.commit({
          repo,
          branch: this.config.branch,
          parentCommit: parent,
          accessToken: this.config.token,
          title,
          description: 'Validated batched profile media from 0xoLemon Launcher.',
          operations
        });
        return;
      } catch (error) {
        lastError = error;
        if (!conflictLike(error)) break;
      }
    }
    throw new SocialError('COVER_PUBLISH_FAILED', `Profile covers could not be published: ${String(lastError && lastError.message || 'unknown error')}`, 503);
  }

  async updatePublisherStats(db, commitCount, bytes) {
    const ref = db.collection('socialSystem').doc('hfCoverPublisher');
    const snapshot = await ref.get();
    const current = snapshot.exists ? snapshot.data() : {};
    const next = {
      commitCountSinceSquash: (Number(current.commitCountSinceSquash) || 0) + commitCount,
      historyBytesEstimate: (Number(current.historyBytesEstimate) || 0) + bytes,
      updatedAtMs: this.now(),
      lastError: null
    };
    await ref.set(next, { merge: true });
    return next;
  }

  async maybeSquash(db, stats) {
    if (!this.config.autoSquash) return;
    if (stats.commitCountSinceSquash < this.config.squashCommitThreshold && stats.historyBytesEstimate < this.config.squashHistoryBytes) return;
    const [namespace, repo] = this.config.repoName.split('/');
    if (!namespace || !repo) throw new SocialError('COVER_REPO_INVALID', 'HF social media repository is invalid.', 503);
    const verifyPublishedPointers = async () => {
      const profiles = await db.collection('socialUsers').limit(5000).get();
      const paths = [...new Set(profiles.docs.map((doc) => String(doc.data().coverPath || '')).filter(Boolean))];
      for (const path of paths) await verifyObject(this.config, path);
    };
    await verifyPublishedPointers();
    const response = await fetch(
      `https://huggingface.co/api/datasets/${encodeURIComponent(namespace)}/${encodeURIComponent(repo)}/super-squash/${encodeURIComponent(this.config.branch)}`,
      {
        method: 'POST',
        headers: { Authorization: `Bearer ${this.config.token}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ message: `Compact 0xoLemon profile media ${new Date(this.now()).toISOString()}` })
      }
    );
    if (!response.ok) {
      await db.collection('socialSystem').doc('hfCoverPublisher').set({ lastError: `HF_SQUASH_${response.status}`, updatedAtMs: this.now() }, { merge: true });
      return;
    }
    const result = await response.json();
    await verifyPublishedPointers();
    await db.collection('socialSystem').doc('hfCoverPublisher').set({
      commitCountSinceSquash: 0,
      historyBytesEstimate: 0,
      lastSquashCommit: String(result.commitId || ''),
      lastSquashAtMs: this.now(),
      lastError: null
    }, { merge: true });
  }

  async processCleanup(tenantId) {
    const db = this.getTenantDb(tenantId);
    const snapshot = await db.collection('socialCoverCleanup')
      .where('dueAtMs', '<=', this.now())
      .limit(this.config.maxOperations)
      .get();
    const deletable = [];
    for (const document of snapshot.docs) {
      const data = document.data();
      const path = String(data.path || '');
      if (!/^covers\/[a-f0-9]{2}\/[a-f0-9]{64}\/[a-f0-9]{64}\.webp$/.test(path)) continue;
      const references = await db.collection('socialUsers').where('coverPath', '==', path).limit(1).get();
      if (references.empty) deletable.push({ document, path });
    }
    if (!deletable.length) return 0;
    await this.commitOperations(
      deletable.map((item) => ({ operation: 'delete', path: item.path })),
      `Retire ${deletable.length} superseded profile cover${deletable.length === 1 ? '' : 's'}`
    );
    for (const item of deletable) await item.document.ref.delete();
    return deletable.length;
  }

  async publishChunk(items) {
    const operations = items.map((item) => ({
      operation: 'addOrUpdate',
      path: item.path,
      content: new Blob([item.buffer], { type: 'image/webp' })
    }));
    await this.commitOperations(operations, `Publish ${items.length} profile cover${items.length === 1 ? '' : 's'}`);
    for (const item of items) await verifyObject(this.config, item.path);

    const byTenant = new Map();
    for (const item of items) {
      if (!byTenant.has(item.tenantId)) byTenant.set(item.tenantId, []);
      byTenant.get(item.tenantId).push(item);
    }
    for (const [tenantId, tenantItems] of byTenant) {
      const db = this.getTenantDb(tenantId);
      for (const item of tenantItems) {
        const profileRef = db.collection('socialUsers').doc(item.userId);
        const queueRef = db.collection('socialCoverQueue').doc(item.accountKey);
        const committed = await db.runTransaction(async (transaction) => {
          const [profileSnapshot, queueSnapshot] = await Promise.all([
            transaction.get(profileRef),
            transaction.get(queueRef)
          ]);
          const queued = queueSnapshot.exists ? queueSnapshot.data() : null;
          if (!queued || queued.hash !== item.hash) return null;
          const current = profileSnapshot.exists ? profileSnapshot.data() : {};
          const revision = (Number(current.coverRevision) || 0) + 1;
          transaction.set(profileRef, {
            coverHash: item.hash,
            coverPath: item.path,
            coverRevision: revision,
            coverUpdatedAtMs: this.now(),
            updatedAtMs: this.now()
          }, { merge: true });
          transaction.delete(queueRef);
          return { current, revision };
        });
        if (!committed) {
          // A newer cover replaced this queue item while its immutable object was
          // being uploaded. Keep the newer queue entry and retire this orphan only
          // after the normal grace period.
          const cleanupId = crypto.createHash('sha256').update(`${tenantId}:${item.path}`).digest('hex');
          await db.collection('socialCoverCleanup').doc(cleanupId).set({
            path: item.path,
            dueAtMs: this.now() + this.config.cleanupGraceMs,
            createdAtMs: this.now()
          });
          continue;
        }
        const { current, revision } = committed;
        if (current.coverPath && current.coverPath !== item.path) {
          const cleanupId = crypto.createHash('sha256').update(`${tenantId}:${current.coverPath}`).digest('hex');
          await db.collection('socialCoverCleanup').doc(cleanupId).set({
            path: current.coverPath,
            dueAtMs: this.now() + this.config.cleanupGraceMs,
            createdAtMs: this.now()
          });
        }
        this.eventHub.publish(tenantId, [], 'cover.published', {
          userId: item.userId,
          hash: item.hash,
          path: item.path,
          revision
        });
      }
      const bytes = tenantItems.reduce((sum, item) => sum + item.buffer.length, 0);
      const stats = await this.updatePublisherStats(db, 1, bytes);
      await this.maybeSquash(db, stats);
    }
  }

  async flush() {
    if (this.flushing || !this.configured()) return { published: 0, pending: this.pending.size };
    this.flushing = true;
    const snapshot = [...this.pending.values()];
    let published = 0;
    try {
      for (let offset = 0; offset < snapshot.length; offset += this.config.maxOperations) {
        const chunk = snapshot.slice(offset, offset + this.config.maxOperations);
        await this.publishChunk(chunk);
        for (const item of chunk) {
          const key = this.queueKey(item.tenantId, item.userId);
          const current = this.pending.get(key);
          if (current && current.hash === item.hash) this.pending.delete(key);
        }
        published += chunk.length;
      }
      let cleaned = 0;
      for (const tenantId of this.hydratedTenants) cleaned += await this.processCleanup(tenantId);
      return { published, cleaned, pending: this.pending.size };
    } finally {
      this.flushing = false;
    }
  }
}

module.exports = { HfCoverPublisher, validateCoverBuffer };
