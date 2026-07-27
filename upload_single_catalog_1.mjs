/**
 * upload_single_catalog_1.mjs  [0xoLemon-1 / xolemon-1]
 * Bản copy của upload_single_catalog.mjs nhưng trỏ vào Firebase project 0xoLemon-1.
 * Dùng để test load balancing trước khi gộp lại.
 * 
 * Cách dùng: node upload_single_catalog_1.mjs <gameId> <tên_thư_mục_trong_assets>
 * Ví dụ: node upload_single_catalog_1.mjs pragmata pragmata
 */

import { initializeApp } from 'firebase/app';
import { getFirestore, doc, setDoc, getDoc } from 'firebase/firestore';
import fs from 'fs';
import path from 'path';

const firebaseConfig = {
  apiKey: "AIzaSyBOeVOoaPMCX6gxnT7UTl_TlCBViBwDxPE",
  authDomain: "xolemon-1.firebaseapp.com",
  projectId: "xolemon-1",
  storageBucket: "xolemon-1.firebasestorage.app",
  messagingSenderId: "813783362435",
  appId: "1:813783362435:web:1c1bbf3d56c082d9d7e4b6"
};
const app = initializeApp(firebaseConfig);
const db  = getFirestore(app);

function readJsonSafe(filePath) {
  if (!fs.existsSync(filePath)) return null;
  try {
    const raw = fs.readFileSync(filePath, 'utf8').replace(/^\uFEFF/, '');
    return JSON.parse(raw);
  } catch (e) {
    console.log(`  ✗ Cannot parse ${path.basename(filePath)}: ${e.message}`);
    return null;
  }
}

function parseVersionLabel(versionStr) {
  const vMatch = versionStr.match(/^(v[\d.]+)/);
  const version = vMatch ? vMatch[1] : versionStr.split(' ')[0];

  const bMatch = versionStr.match(/\(Build (\d+)\)/);
  const buildId = bMatch ? bMatch[1] : version.replace(/[^0-9]/g, '');

  const label = bMatch ? `${version} (Build ${buildId})` : version;
  return { version, buildId, label };
}

function parseVersions(catalog) {
  if (!catalog?.versions?.length) return [];
  const latestRaw = catalog.latestVersion ?? catalog.latest_version ?? '';

  return catalog.versions.map(v => {
    const { version, buildId, label } = parseVersionLabel(v.version);
    return {
      version,
      label,
      buildId,
      sizeBytes: v.totalSize ?? v.total_size ?? v.sizeBytes ?? 49_690_000_000,
      latest: v.version === latestRaw || version === latestRaw,
    };
  });
}

function buildSummary(gameId, meta, cloudSave, versions) {
  const rawTitle   = meta?.title ?? '';
  const title = rawTitle
    ? rawTitle.replace(/\b\w/g, c => c.toUpperCase())
    : gameId;

  const dev  = typeof meta?.developers === 'string'
    ? meta.developers
    : (Array.isArray(meta?.developers) ? meta.developers[0] : '') ?? '';
  const pub  = typeof meta?.publishers === 'string'
    ? meta.publishers
    : (Array.isArray(meta?.publishers) ? meta.publishers[0] : '') ?? '';

  const genres = (() => {
    if (!meta?.genres) return [];
    if (typeof meta.genres === 'string') return meta.genres.split(',').map(g => g.trim()).filter(Boolean);
    if (Array.isArray(meta.genres)) return meta.genres;
    return [];
  })();

  const releaseDate = meta?.releaseDate?.date ?? meta?.release_date ?? '';
  const shortDescription = meta?.shortDescription ?? meta?.short_description ?? '';

  const latestVersion = versions.find(v => v.latest)?.version
    ?? versions[versions.length - 1]?.version ?? '';

  const STORE_ROOT = 'E:\\0xoLemon store';
  const folderName = title.replace(/[<>:"/\\|?*™®]/g, '').replace(/\s+/g, ' ').trim() || gameId;
  const launchExe  = gameId === '007-first-light'
    ? 'Retail\\007FirstLight.exe'
    : `${folderName}.exe`;

  return {
    id: gameId,
    title,
    subtitle: dev,
    developer: dev,
    publisher: pub,
    releaseDate,
    genres,
    shortDescription,
    latestVersion,
    availableVersions: versions,
    // default fields, will be merged
    gridAssetId: '',
    heroAssetId: '',
    logoAssetId: '',
    iconAssetId: '',
    install: {
      defaultStoreRoot: STORE_ROOT,
      defaultInstallFolder: `${STORE_ROOT}\\common\\${folderName}`,
      defaultDownloadingFolder: `${STORE_ROOT}\\downloading\\${folderName}`,
      storageLabel: 'SSD',
      supportsResume: true,
      launchExecutable: launchExe,
    },
    cloudSave: cloudSave ?? { enabled: false, saveRoots: [], include: [], exclude: [] },
    assetPackPath: `assets/games/${gameId}/core.0xo`,
  };
}

async function main() {
  const args = process.argv.slice(2);
  if (args.length < 2) {
    console.error("Cách dùng: node upload_single_catalog.mjs <gameId> <tên_thư_mục_trong_assets>");
    console.error("Ví dụ: node upload_single_catalog.mjs pragmata pragmata");
    process.exit(1);
  }

  const gameId = args[0];
  const folder = args[1];

  console.log(`\nĐang build catalog entry cho game: ${gameId}`);

  const metaPath      = path.join('src', 'assets', folder, 'details', 'metadata', 'game-detail.normalized.json');
  const cloudSavePath = path.join('src', 'assets', folder, 'details', 'metadata', 'cloud-save.json');
  const depotCatalogPath = path.join('depot', gameId, 'catalog.json');

  const meta      = readJsonSafe(metaPath);
  const cloudSave = readJsonSafe(cloudSavePath);
  const depotCat  = readJsonSafe(depotCatalogPath);

  if (!meta) {
    console.log(`  ! Không tìm thấy ${metaPath}.`);
  }
  if (!depotCat) {
    console.log(`  ! Không tìm thấy ${depotCatalogPath}. Dừng lại.`);
    process.exit(1);
  }

  const versions = parseVersions(depotCat);
  const summary = buildSummary(gameId, meta, cloudSave, versions);

  console.log(`  ✓ Title: ${summary.title}`);
  console.log(`  ✓ Versions: ${versions.map(v => v.version).join(', ')}`);

  const docRef = doc(db, 'config', 'gameCatalog');
  const existingSnap = await getDoc(docRef);
  const catalog = existingSnap.exists() ? existingSnap.data() : { games: [] };

  const existingIndex = catalog.games.findIndex(g => g.id === gameId);
  
  // Merge để không làm mất các custom fields nếu game đã tồn tại
  if (existingIndex !== -1) {
    const existing = catalog.games[existingIndex];
    if (existing.gridAssetId) summary.gridAssetId = existing.gridAssetId;
    if (existing.heroAssetId) summary.heroAssetId = existing.heroAssetId;
    if (existing.logoAssetId) summary.logoAssetId = existing.logoAssetId;
    if (existing.iconAssetId) summary.iconAssetId = existing.iconAssetId;
    if (existing.assets_override) summary.assets_override = existing.assets_override;
    
    catalog.games[existingIndex] = { ...existing, ...summary };
    console.log(`  ✓ Đã cập nhật (merge) vào catalog cho game hiện tại: ${gameId}`);
  } else {
    catalog.games.push(summary);
    console.log(`  ✓ Đã thêm mới game ${gameId} vào catalog.`);
  }

  console.log(`  → Đang ghi lại lên Firestore (Tổng cộng ${catalog.games.length} games)...`);
  await setDoc(docRef, catalog);
  
  console.log(`\n✅ Tuyệt đối an toàn: Chỉ update/insert game ${gameId}, không chạm vào bất kỳ game nào khác!`);
  process.exit(0);
}

main().catch(err => { console.error(err); process.exit(1); });
