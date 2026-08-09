const GAME_ID = 'ea-sports-fc-26';

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

function integerEnv(name, fallback, minimum = 1) {
  const value = Number.parseInt(process.env[name] || '', 10);
  return Number.isFinite(value) && value >= minimum ? value : fallback;
}

function parseList(value, fallback = []) {
  if (!value) return fallback;
  return value.split(',').map((item) => item.trim()).filter(Boolean);
}

function parsePackageFiles(value) {
  if (!value) return [];
  let parsed;
  try {
    parsed = JSON.parse(value);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  return parsed.map((entry) => {
    if (!entry || typeof entry.path !== 'string') return null;
    const normalizedPath = entry.path.replace(/\\/g, '/');
    const pathParts = normalizedPath.split('/');
    const safePath = normalizedPath.length > 0 &&
      normalizedPath.length <= 240 &&
      !normalizedPath.startsWith('/') &&
      !normalizedPath.includes(':') &&
      pathParts.every((part) => part && part !== '.' && part !== '..');
    if (!safePath || !/^[a-f0-9]{64}$/i.test(entry.sha256 || '')) return null;
    if (!Number.isSafeInteger(Number(entry.sizeBytes)) || Number(entry.sizeBytes) < 0) return null;
    return {
      path: normalizedPath,
      sha256: entry.sha256.toLowerCase(),
      sizeBytes: Number(entry.sizeBytes)
    };
  }).filter(Boolean);
}

function loadActivationConfig() {
  const packageSize = Number.parseInt(process.env.ACTIVATION_PACKAGE_SIZE_BYTES || '', 10);
  const packageSha256 = (process.env.ACTIVATION_PACKAGE_SHA256 || '').trim().toLowerCase();
  const packageFiles = parsePackageFiles(process.env.ACTIVATION_PACKAGE_FILES_JSON);
  const encryptionKey = (process.env.ACTIVATION_ENCRYPTION_KEY || '').trim();

  const config = {
    gameId: GAME_ID,
    tenantId: process.env.ACTIVATION_TENANT_ID || '0xolemon',
    capacity: 5,
    windowMs: 10 * 60 * 60 * 1000,
    cooldownMs: 96 * 60 * 60 * 1000,
    reservationMs: integerEnv('ACTIVATION_RESERVATION_SECONDS', 180, 30) * 1000,
    accountRateWindowMs: 15 * 60 * 1000,
    accountRateMax: integerEnv('ACTIVATION_ACCOUNT_RATE_MAX', 8, 1),
    discord: {
      apiBase: 'https://discord.com/api/v10',
      guildId: process.env.DISCORD_REQUIRED_GUILD_ID || '1492076309323714570',
      allowedRoleIds: parseList(process.env.DISCORD_ALLOWED_ROLE_IDS, DEFAULT_ROLE_IDS),
      minimumAccountAgeMs: integerEnv('DISCORD_MINIMUM_ACCOUNT_AGE_DAYS', 7, 1) * 24 * 60 * 60 * 1000,
      timeoutMs: integerEnv('DISCORD_API_TIMEOUT_MS', 10000, 1000)
    },
    ea: {
      clientId: process.env.EA_CLIENT_ID || 'JUNO_PC_CLIENT',
      clientSecret: (process.env.EA_CLIENT_SECRET || '').trim(),
      machineHash: (process.env.EA_MACHINE_HASH || '').trim(),
      bootstrapRefreshToken: (process.env.EA_REFRESH_TOKEN_BOOTSTRAP || '').trim(),
      tokenUrl: 'https://accounts.ea.com/connect/token',
      licenseUrl: 'https://proxy.novafusion.ea.com/licenses',
      timeoutMs: integerEnv('EA_API_TIMEOUT_MS', 30000, 3000)
    },
    package: {
      version: (process.env.ACTIVATION_PACKAGE_VERSION || '').trim(),
      url: (process.env.ACTIVATION_PACKAGE_URL || '').trim(),
      sizeBytes: Number.isSafeInteger(packageSize) && packageSize > 0 ? packageSize : 0,
      sha256: /^[a-f0-9]{64}$/.test(packageSha256) ? packageSha256 : '',
      files: packageFiles,
      archivePassword: process.env.ACTIVATION_PACKAGE_PASSWORD || ''
    },
    encryptionKey,
    adminKey: process.env.ACTIVATION_ADMIN_KEY || '',
    minimumLauncherVersion: (process.env.ACTIVATION_MIN_LAUNCHER_VERSION || '').trim()
  };

  const readinessErrors = [];
  if (!config.encryptionKey) readinessErrors.push('ACTIVATION_ENCRYPTION_KEY');
  if (!config.ea.clientSecret) readinessErrors.push('EA_CLIENT_SECRET');
  if (!config.ea.machineHash) readinessErrors.push('EA_MACHINE_HASH');
  if (!config.ea.bootstrapRefreshToken) readinessErrors.push('EA_REFRESH_TOKEN_BOOTSTRAP');
  if (!config.package.version) readinessErrors.push('ACTIVATION_PACKAGE_VERSION');
  if (!config.package.url) readinessErrors.push('ACTIVATION_PACKAGE_URL');
  if (!config.package.sizeBytes) readinessErrors.push('ACTIVATION_PACKAGE_SIZE_BYTES');
  if (!config.package.sha256) readinessErrors.push('ACTIVATION_PACKAGE_SHA256');
  if (!config.package.files.length) readinessErrors.push('ACTIVATION_PACKAGE_FILES_JSON');

  config.readiness = {
    ready: readinessErrors.length === 0,
    missingConfiguration: readinessErrors
  };
  return config;
}

module.exports = { GAME_ID, loadActivationConfig };
