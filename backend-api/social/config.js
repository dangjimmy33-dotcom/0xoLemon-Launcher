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

function listEnv(name, fallback = []) {
  const value = String(process.env[name] || '').trim();
  return value ? value.split(',').map((item) => item.trim()).filter(Boolean) : fallback;
}

function intEnv(name, fallback, minimum = 1) {
  const parsed = Number.parseInt(process.env[name] || '', 10);
  return Number.isSafeInteger(parsed) && parsed >= minimum ? parsed : fallback;
}

function boolEnv(name, fallback = false) {
  const value = String(process.env[name] || '').trim().toLowerCase();
  if (!value) return fallback;
  return value === '1' || value === 'true' || value === 'yes';
}

function loadSocialConfig() {
  return {
    enabled: boolEnv('SOCIAL_ENABLED', false),
    canaryMode: boolEnv('SOCIAL_CANARY_MODE', true),
    canaryDiscordIds: new Set(listEnv('SOCIAL_CANARY_DISCORD_IDS')),
    accountHmacKey: String(process.env.SOCIAL_ACCOUNT_HMAC_KEY || '').trim(),
    discord: {
      apiBase: 'https://discord.com/api/v10',
      guildId: process.env.DISCORD_REQUIRED_GUILD_ID || '1492076309323714570',
      allowedRoleIds: listEnv('DISCORD_ALLOWED_ROLE_IDS', DEFAULT_ROLE_IDS),
      minimumAccountAgeMs: intEnv('DISCORD_MINIMUM_ACCOUNT_AGE_DAYS', 7) * 24 * 60 * 60 * 1000,
      timeoutMs: intEnv('DISCORD_API_TIMEOUT_MS', 10000, 1000)
    },
    presence: {
      heartbeatMs: intEnv('SOCIAL_PRESENCE_HEARTBEAT_SECONDS', 45, 15) * 1000,
      staleMs: intEnv('SOCIAL_PRESENCE_STALE_SECONDS', 120, 45) * 1000
    },
    cover: {
      repoName: String(process.env.HF_SOCIAL_MEDIA_REPO || 'PROBBI/PROBBINE').trim(),
      branch: String(process.env.HF_SOCIAL_MEDIA_BRANCH || 'main').trim(),
      token: String(process.env.HF_SOCIAL_MEDIA_TOKEN || '').trim(),
      batchMs: intEnv('SOCIAL_COVER_BATCH_SECONDS', 600, 30) * 1000,
      maxBytes: intEnv('SOCIAL_COVER_MAX_BYTES', 256 * 1024, 32 * 1024),
      maxOperations: intEnv('SOCIAL_COVER_COMMIT_OPERATIONS', 75, 1),
      cleanupGraceMs: intEnv('SOCIAL_COVER_CLEANUP_HOURS', 24, 1) * 60 * 60 * 1000,
      squashCommitThreshold: intEnv('SOCIAL_COVER_SQUASH_COMMITS', 5000, 100),
      squashHistoryBytes: intEnv('SOCIAL_COVER_SQUASH_HISTORY_BYTES', 2 * 1024 * 1024 * 1024, 64 * 1024 * 1024),
      autoSquash: boolEnv('SOCIAL_COVER_AUTO_SQUASH', true)
    }
  };
}

module.exports = { loadSocialConfig };
