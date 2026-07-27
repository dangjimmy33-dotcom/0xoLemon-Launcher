/**
 * upload_single_detail_1.mjs  [0xoLemon-1 / xolemon-1]
 * Bản copy của upload_single_detail.mjs nhưng trỏ vào Firebase project 0xoLemon-1.
 * Dùng để test load balancing trước khi gộp lại.
 * 
 * Cách dùng: node upload_single_detail_1.mjs <gameId> <tên_thư_mục_trong_assets>
 * Ví dụ: node upload_single_detail_1.mjs pragmata pragmata
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
    console.log(`  ✗ Parse error ${path.basename(filePath)}: ${e.message}`);
    return null;
  }
}

async function main() {
  const args = process.argv.slice(2);
  if (args.length < 2) {
    console.error("Cách dùng: node upload_single_detail.mjs <gameId> <tên_thư_mục_trong_assets>");
    console.error("Ví dụ: node upload_single_detail.mjs pragmata pragmata");
    process.exit(1);
  }

  const gameId = args[0];
  const folder = args[1];

  console.log(`\nĐang xử lý chi tiết cho game: ${gameId} (Thư mục: ${folder})`);

  // Lấy thông tin summary từ gameCatalog nếu có
  const catalogSnap = await getDoc(doc(db, 'config', 'gameCatalog'));
  const gameCatalog = catalogSnap.exists() ? catalogSnap.data() : { games: [] };
  const summary = gameCatalog.games?.find(g => g.id === gameId);

  const detailsPath = path.join('src', 'assets', folder, 'details', 'metadata');
  const metaPath = path.join(detailsPath, 'game-detail.normalized.json');
  const mediaPath = path.join(detailsPath, 'media-manifest.json');
  const achievePath = path.join(detailsPath, 'steam-achievements.raw.json');
  
  const meta = readJsonSafe(metaPath);
  if (!meta) {
    console.log(`  ! Không tìm thấy hoặc lỗi parse ${metaPath}. Bạn có chắc thư mục đúng không?`);
  }

  const mediaRaw = readJsonSafe(mediaPath) || [];
  const achieveRaw = readJsonSafe(achievePath);

  const title = meta?.title ?? summary?.title ?? gameId;
  const shortDesc = meta?.shortDescription ?? summary?.shortDescription ?? 'Game details are packaged for the desktop launcher.';
  const detailedDesc = meta?.detailedDescriptionHtml ?? meta?.detailedDescription ?? shortDesc;

  // Build Media with correct ID scheme
  let videoIdx = 0;
  let screenshotIdx = 0;
  const thumbByTitle = {};

  for (const m of mediaRaw) {
    if ((m.role === 'video-thumbnail' || m.role === 'video-poster') && m.sourceUrl) {
      thumbByTitle[m.title] = thumbByTitle[m.title] || m.sourceUrl;
    }
  }

  const media = [];
  for (const m of mediaRaw) {
    if (!m.sourceUrl) continue;
    if (m.role === 'video') {
      const idx = videoIdx++;
      media.push({ id: `movie-${idx}`, role: 'video', title: m.title || `Video ${idx+1}`, mimeType: 'video/mp4', assetId: m.sourceUrl });
      const thumbUrl = thumbByTitle[m.title];
      if (thumbUrl) {
        media.push({ id: `movie-thumb-${idx}`, role: 'video-thumb', title: m.title || `Video ${idx+1}`, mimeType: 'image/jpeg', assetId: thumbUrl });
      }
    } else if (m.role === 'screenshot' || m.role === 'gif') {
      media.push({ id: `screenshot-${screenshotIdx++}`, role: m.role, title: m.title || m.role, mimeType: 'image/jpeg', assetId: m.sourceUrl });
    }
  }

  // Build Achievements
  const achievements = [];
  if (achieveRaw?.game?.availableGameStats?.achievements) {
    for (const a of achieveRaw.game.availableGameStats.achievements) {
      achievements.push({
        id: a.name,
        title: a.displayName || a.name,
        description: a.description || (a.hidden === 1 ? 'Hidden' : ''),
        hidden: a.hidden === 1,
        iconAssetId: a.icon || '',
        iconGrayAssetId: a.icongray || ''
      });
    }
  }

  const detailObj = {
    gameId,
    locale: 'en-US',
    title,
    shortDescription: shortDesc,
    detailedDescription: detailedDesc,
    developers: typeof meta?.developers === 'string' ? [meta.developers] : (meta?.developers || []),
    publishers: typeof meta?.publishers === 'string' ? [meta.publishers] : (meta?.publishers || []),
    releaseDate: meta?.releaseDate?.date || summary?.releaseDate || '',
    genres: summary?.genres || [],
    categories: meta?.categories || [],
    ratings: meta?.metacritic ? [{ source: 'metacritic', score: meta.metacritic.score.toString() }] : [],
    media,
    achievements,
    sounds: [],
    install: summary?.install ?? null,
    cloudSave: summary?.cloudSave ?? null,
    descriptionImages: [],
    versions: summary?.availableVersions ?? [],
    metadataSource: 'firestore'
  };

  console.log(`  ✓ Đã đóng gói dữ liệu: ${media.length} hình/video, ${achievements.length} achievements`);
  console.log(`  ✓ Đang tải lên Firestore collection 'gameDetails', doc: ${gameId}...`);
  
  await setDoc(doc(db, 'gameDetails', gameId), detailObj, { merge: true });

  console.log(`\n✅ Đã tải lên thành công dữ liệu cho game ${gameId}!`);
  process.exit(0);
}

main().catch(err => { console.error(err); process.exit(1); });
