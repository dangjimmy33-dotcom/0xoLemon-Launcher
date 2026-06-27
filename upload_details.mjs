/**
 * upload_details.mjs
 * Đọc toàn bộ metadata từ src/assets (detail HTML, media manifest, achievements)
 * và upload thành collection `gameDetails` lên Firestore.
 * Chạy: node upload_details.mjs
 */

import { initializeApp } from 'firebase/app';
import { getFirestore, doc, setDoc } from 'firebase/firestore';
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

// gameId → folder name
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
  const catalogSnap = await import('firebase/firestore').then(({ getDoc }) => getDoc(doc(db, 'config', 'gameCatalog')));
  const gameCatalog = catalogSnap.exists() ? catalogSnap.data() : { games: [] };
  
  for (const [gameId, folder] of Object.entries(FOLDER_MAP)) {
    console.log(`\nProcessing details for: ${gameId}`);
    const summary = gameCatalog.games?.find(g => g.id === gameId);
    
    const detailsPath = path.join('src', 'assets', folder, 'details', 'metadata');
    const metaPath = path.join(detailsPath, 'game-detail.normalized.json');
    const mediaPath = path.join(detailsPath, 'media-manifest.json');
    const achievePath = path.join(detailsPath, 'steam-achievements.raw.json');
    
    const meta = readJsonSafe(metaPath);
    const mediaRaw = readJsonSafe(mediaPath) || [];
    const achieveRaw = readJsonSafe(achievePath);

    const title = meta?.title ?? summary?.title ?? gameId;
    const shortDesc = meta?.shortDescription ?? summary?.shortDescription ?? 'Game details are packaged for the desktop launcher.';
    const detailedDesc = meta?.detailedDescriptionHtml ?? meta?.detailedDescription ?? shortDesc;

    // Build Media with correct ID scheme:
    // - videos: id = "movie-N"  (role = 'video')
    // - video thumbs: id = "movie-thumb-N"  (role = 'video-thumb') → maps to "movie-N"
    // - screenshots/gifs: id = "screenshot-N"
    let videoIdx = 0;
    let screenshotIdx = 0;
    const thumbByTitle = {};

    // First pass: collect thumbnail URLs keyed by title
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
        // Also emit the thumbnail if we have one
        const thumbUrl = thumbByTitle[m.title];
        if (thumbUrl) {
          media.push({ id: `movie-thumb-${idx}`, role: 'video-thumb', title: m.title || `Video ${idx+1}`, mimeType: 'image/jpeg', assetId: thumbUrl });
        }
      } else if (m.role === 'screenshot' || m.role === 'gif') {
        media.push({ id: `screenshot-${screenshotIdx++}`, role: m.role, title: m.title || m.role, mimeType: 'image/jpeg', assetId: m.sourceUrl });
      }

      // Skip store_art roles (header, capsule, etc.) – those go in gameCatalog/assets_override
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

    console.log(`  ✓ Built detail: ${media.length} media items, ${achievements.length} achievements`);
    
    // Upload to Firestore collection 'gameDetails' (Merge with existing so custom fields aren't lost)
    await setDoc(doc(db, 'gameDetails', gameId), detailObj, { merge: true });
  }

  console.log(`\n✅ Done uploading full details for ${Object.keys(FOLDER_MAP).length} games.`);
  process.exit(0);
}

main().catch(err => { console.error(err); process.exit(1); });
