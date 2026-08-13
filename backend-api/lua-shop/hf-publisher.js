const { LuaShopError } = require('./quota');

const DEFAULT_REPO = 'Immaking/Luas';

function publisherConfig() {
  return {
    token: String(process.env.HF_LUA_COMMUNITY_WRITE_TOKEN || '').trim(),
    repoName: String(process.env.HF_LUA_COMMUNITY_REPO || DEFAULT_REPO).trim(),
    branch: String(process.env.HF_LUA_COMMUNITY_BRANCH || 'main').trim()
  };
}

async function currentParentCommit(repoName, branch, token) {
  const response = await fetch(
    `https://huggingface.co/api/datasets/${encodeURIComponent(repoName)}/revision/${encodeURIComponent(branch)}`,
    { headers: { Authorization: `Bearer ${token}` } }
  );
  if (!response.ok) throw new Error(`HF_REPO_INFO_${response.status}`);
  const payload = await response.json();
  if (typeof payload.sha !== 'string' || !/^[a-f0-9]{40,64}$/i.test(payload.sha)) {
    throw new Error('HF_REPO_INFO_INVALID');
  }
  return payload.sha;
}

async function readCommunityIndex(repoName, branch, token, appid) {
  const path = `community/index/${appid}.json`;
  const response = await fetch(
    `https://huggingface.co/datasets/${repoName}/resolve/${encodeURIComponent(branch)}/${path}`,
    { headers: { Authorization: `Bearer ${token}`, 'Cache-Control': 'no-cache' } }
  );
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`HF_INDEX_READ_${response.status}`);
  const payload = await response.json();
  return payload && typeof payload === 'object' ? payload : null;
}

function mergeIndex(current, packageInfo) {
  const packagePath = `community/packages/${packageInfo.appid}/${packageInfo.revision}.zip`;
  const revisions = Array.isArray(current && current.revisions) ? [...current.revisions] : [];
  if (!revisions.some((entry) => entry && entry.revision === packageInfo.revision)) {
    revisions.push({
      revision: packageInfo.revision,
      packagePath,
      sizeBytes: packageInfo.sizeBytes,
      addedAt: new Date().toISOString()
    });
  }
  return {
    schemaVersion: 1,
    appid: packageInfo.appid,
    latestRevision: packageInfo.revision,
    packagePath,
    updatedAt: new Date().toISOString(),
    revisions
  };
}

function conflictLike(error) {
  const status = Number(error && (error.statusCode || error.status));
  const message = String(error && error.message || '');
  return status === 409 || /409|conflict|parent commit/i.test(message);
}

async function publishCommunityPackage(packageBuffer, packageInfo, accountKey, requestId) {
  const config = publisherConfig();
  if (!config.token || !config.repoName) {
    throw new LuaShopError('COMMUNITY_PUBLISH_NOT_CONFIGURED', 'Community publishing is not configured.', 503);
  }
  const hub = require('@huggingface/hub');
  const repo = { type: 'dataset', name: config.repoName };
  const packagePath = `community/packages/${packageInfo.appid}/${packageInfo.revision}.zip`;

  if (await hub.fileExists({ repo, path: packagePath, revision: config.branch, accessToken: config.token })) {
    return { status: 'exists', packagePath, revision: packageInfo.revision };
  }

  let lastError;
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      const [parentCommit, currentIndex] = await Promise.all([
        currentParentCommit(config.repoName, config.branch, config.token),
        readCommunityIndex(config.repoName, config.branch, config.token, packageInfo.appid)
      ]);
      const index = mergeIndex(currentIndex, packageInfo);
      await hub.commit({
        repo,
        branch: config.branch,
        parentCommit,
        accessToken: config.token,
        title: `Cache Lua package ${packageInfo.appid} ${packageInfo.revision.slice(0, 12)}`,
        description: 'Validated community cache contribution from 0xoLemon Launcher.',
        operations: [
          {
            operation: 'addOrUpdate',
            path: packagePath,
            content: new Blob([packageBuffer], { type: 'application/zip' })
          },
          {
            operation: 'addOrUpdate',
            path: `community/index/${packageInfo.appid}.json`,
            content: new Blob([JSON.stringify(index, null, 2) + '\n'], { type: 'application/json' })
          }
        ]
      });
      return { status: 'published', packagePath, revision: packageInfo.revision };
    } catch (error) {
      lastError = error;
      if (!conflictLike(error)) break;
      if (await hub.fileExists({ repo, path: packagePath, revision: config.branch, accessToken: config.token })) {
        return { status: 'exists', packagePath, revision: packageInfo.revision };
      }
    }
  }
  throw new LuaShopError(
    'COMMUNITY_PUBLISH_FAILED',
    `Community package could not be published: ${String(lastError && lastError.message || 'unknown error')}`,
    503
  );
}

module.exports = { mergeIndex, publishCommunityPackage };
