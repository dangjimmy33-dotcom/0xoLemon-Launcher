/**
 * upload_full_catalog.mjs
 * Đọc metadata từ local JSON + versions từ HuggingFace, upload lên Firestore.
 * Chạy: node upload_full_catalog.mjs
 */

import { initializeApp } from 'firebase/app';
import { getFirestore, doc, setDoc, getDoc } from 'firebase/firestore';
import fs from 'fs';
import path from 'path';

const firebaseConfig = {
  apiKey: "AIzaSyAdqkMDJTjeVAfnwd8twlF2dvrKpWdNlUY",
  authDomain: "xolemon-b360e.firebaseapp.com",
  projectId: "xolemon-b360e",
  storageBucket: "xolemon-b360e.firebasestorage.app",
  messagingSenderId: "527817093688",
  appId: "1:527817093688:web:a44ad0eb11c05a7b95c07c"
};
const app = initializeApp(firebaseConfig);
const db  = getFirestore(app);


// HuggingFace tokens loaded from huggingface-repos.json (gitignored — never commit tokens)
const HF_REPOS = JSON.parse(fs.readFileSync('src-tauri/huggingface-repos.json', 'utf8'));


// gameId → tên thư mục trong src/assets
const FOLDER_MAP = {
  '007-first-light':                                         '007 first light',
  'among-us':                                               'Among Us',
  'ea-sports-fc-26':                                        'EA SPORTS FC™ 26',
  'geometry-dash':                                          'Geometry Dash',
  'hello-kitty-island-adventure':                           'Hello Kitty Island Adventure',
  'meccha-chameleon':                                       'MECCHA CHAMELEON',
  'microsoft-flight-simulator-2020-40th-anniversary-edition': 'Microsoft Flight Simulator (2020) 40th Anniversary Edition',
  'octopath-traveler-0':                                    'OCTOPATH TRAVELER 0',
  'persona-5-royal':                                        'Persona 5 Royal',
  'persona-3-reload':                                       'persona-3-reload',
  'solo-leveling-arise-overdrive':                          'Solo Leveling ARISE OVERDRIVE',
  'stellar-blade':                                          'Stellar Blade',
  'tom-clancy-s-splinter-cell-blacklist':                   "Tom Clancy’s Splinter Cell Blacklist",
};

// Đọc JSON, bỏ BOM nếu có
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

// Fetch catalog.json từ HuggingFace cho một gameId
async function fetchHFCatalog(gameId) {
  for (const { repoId, token } of HF_REPOS.repositories) {
    const url = `https://huggingface.co/datasets/${repoId}/resolve/main/${gameId}/catalog.json`;
    try {
      const res = await fetch(url, {
        headers: { Authorization: `Bearer ${token}` },
        signal: AbortSignal.timeout(10000),
      });
      if (!res.ok) continue;
      const data = await res.json();
      if (data?.versions?.length > 0) {
        console.log(`  ✓ HF versions in ${repoId}`);
        return data;
      }
    } catch { /* try next */ }
  }
  return null;
}

// Parse phiên bản từ HF catalog
// version string có thể là: "v2.16.2 (Build 23704290) - Uploaded 2026-06-18"
function parseVersionLabel(versionStr) {
  // Tách version: lấy phần đầu trước dấu cách
  const vMatch = versionStr.match(/^(v[\d.]+)/);
  const version = vMatch ? vMatch[1] : versionStr.split(' ')[0];

  // Tách buildId từ "(Build XXXXXXXX)"
  const bMatch = versionStr.match(/\(Build (\d+)\)/);
  const buildId = bMatch ? bMatch[1] : version.replace(/[^0-9]/g, '');

  // Label đẹp hơn
  const label = bMatch ? `${version} (Build ${buildId})` : version;

  return { version, buildId, label };
}

function parseVersions(catalog) {
  if (!catalog?.versions?.length) return [];
  // latestVersion có thể là string version đầy đủ
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

// Build GameSummary đầy đủ
function buildSummary(gameId, meta, cloudSave, versions) {
  const rawTitle   = meta?.title ?? '';
  // Capitalize đầu mỗi từ nếu title bị lowercase
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
    // asset URLs đến từ assets_override (SteamGridDB), để trống ở đây
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
  const games = [];

  for (const [gameId, folder] of Object.entries(FOLDER_MAP)) {
    console.log(`\nProcessing: ${gameId}`);

    const metaPath      = path.join('src', 'assets', folder, 'details', 'metadata', 'game-detail.normalized.json');
    const cloudSavePath = path.join('src', 'assets', folder, 'details', 'metadata', 'cloud-save.json');

    const meta      = readJsonSafe(metaPath);
    const cloudSave = readJsonSafe(cloudSavePath);

    if (!meta) {
      console.log(`  ✗ No metadata (${metaPath})`);
    } else {
      console.log(`  ✓ Meta: "${meta.title}" | dev="${meta.developers}" pub="${meta.publishers}"`);
    }

    const hfCatalog = await fetchHFCatalog(gameId);
    const versions  = parseVersions(hfCatalog);
    if (versions.length > 0) {
      console.log(`  ✓ Versions: ${versions.map(v => `${v.version}(${v.buildId})`).join(', ')}`);
    } else {
      console.log('  ✗ No versions on HuggingFace');
    }

    const summary = buildSummary(gameId, meta, cloudSave, versions);
    console.log(`  → ${summary.title} | Dev: "${summary.developer}" | Latest: "${summary.latestVersion}"`);
    games.push(summary);
  }

  const docRef = doc(db, 'config', 'gameCatalog');
  const existingSnap = await getDoc(docRef);
  const existingCatalog = existingSnap.exists() ? existingSnap.data() : { games: [] };
  const existingGamesMap = new Map((existingCatalog.games || []).map(g => [g.id, g]));

  // Merge new generated summaries into existing map
  for (const summary of games) {
    const existing = existingGamesMap.get(summary.id);
    if (existing) {
      // Preserve custom manual configurations from Firestore
      summary.gridAssetId = existing.gridAssetId || summary.gridAssetId;
      summary.heroAssetId = existing.heroAssetId || summary.heroAssetId;
      summary.logoAssetId = existing.logoAssetId || summary.logoAssetId;
      summary.iconAssetId = existing.iconAssetId || summary.iconAssetId;
      summary.assets_override = existing.assets_override || summary.assets_override;
      
      // Merge all new data into the existing object to preserve any other custom fields
      existingGamesMap.set(summary.id, { ...existing, ...summary });
    } else {
      existingGamesMap.set(summary.id, summary);
    }
  }

  const catalog = { 
    ...existingCatalog, 
    defaultLocale: 'en-US', 
    games: Array.from(existingGamesMap.values()) 
  };
  
  console.log('\n\nUploading to Firestore config/gameCatalog (Merged with existing data)...');
  await setDoc(docRef, catalog);
  console.log(`✅ Done! Uploaded/Updated ${catalog.games.length} games.`);
  process.exit(0);
}

main().catch(err => { console.error(err); process.exit(1); });
