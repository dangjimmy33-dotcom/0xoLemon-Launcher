import { initializeApp } from 'firebase/app';
import { getFirestore, doc, getDoc, setDoc } from 'firebase/firestore';

const firebaseConfig = {
  apiKey: "AIzaSyAdqkMDJTjeVAfnwd8twlF2dvrKpWdNlUY",
  authDomain: "xolemon-b360e.firebaseapp.com",
  projectId: "xolemon-b360e",
  storageBucket: "xolemon-b360e.firebasestorage.app",
  messagingSenderId: "330469620392",
  appId: "1:330469620392:web:ad6f6e9288820f18ef209d",
  measurementId: "G-FZTWK4JCKG"
};

const app = initializeApp(firebaseConfig);
const db = getFirestore(app);

const SGDB_API_KEY = "6949533daea9444b0e8f2dfe121a0c30";

async function fetchSgdb(endpoint) {
  const res = await fetch(`https://www.steamgriddb.com/api/v2/${endpoint}`, {
    headers: { Authorization: `Bearer ${SGDB_API_KEY}` }
  });
  const json = await res.json();
  if (!json.success) return null;
  return json.data;
}

async function searchGame(title) {
  // Try clean title
  const cleanTitle = title.replace(/edition|anniversary|overdrive/gi, '').trim();
  const data = await fetchSgdb(`search/autocomplete/${encodeURIComponent(cleanTitle)}`);
  if (data && data.length > 0) return data[0].id;
  return null;
}

async function getAsset(type, gameId, params = '') {
  const data = await fetchSgdb(`${type}/game/${gameId}${params}`);
  if (data && data.length > 0) return data[0].url;
  return null;
}

async function run() {
  const catalogRef = doc(db, 'config', 'gameCatalog');
  const snap = await getDoc(catalogRef);
  if (!snap.exists()) {
    console.error("No catalog found!");
    process.exit(1);
  }

  const catalog = snap.data();
  const overrideRef = doc(db, 'config', 'assets_override');
  const overrideSnap = await getDoc(overrideRef);
  const overrides = overrideSnap.exists() ? overrideSnap.data() : {};

  let updatedCount = 0;

  for (const game of catalog.games) {
    console.log(`Processing: ${game.title} (${game.id})`);
    
    // Check if overrides already exist for this game to save requests
    if (overrides[game.gridAssetId] && overrides[game.heroAssetId]) {
      console.log(`  -> Already has assets, skipping.`);
      continue;
    }

    const sgdbId = await searchGame(game.title);
    if (!sgdbId) {
      console.log(`  -> Not found on SteamGridDB!`);
      continue;
    }

    console.log(`  -> Found SGDB ID: ${sgdbId}`);
    
    const grid = await getAsset('grids', sgdbId, '?dimensions=600x900');
    const hero = await getAsset('heroes', sgdbId);
    const logo = await getAsset('logos', sgdbId);
    const icon = await getAsset('icons', sgdbId);

    if (grid) overrides[game.gridAssetId] = grid;
    if (hero) overrides[game.heroAssetId] = hero;
    if (logo) overrides[game.logoAssetId] = logo;
    if (icon) overrides[game.iconAssetId] = icon;

    console.log(`  -> Updated: Grid=${!!grid}, Hero=${!!hero}, Logo=${!!logo}, Icon=${!!icon}`);
    updatedCount++;
    
    // Rate limit delay
    await new Promise(r => setTimeout(r, 500));
  }

  if (updatedCount > 0) {
    console.log('Saving overrides to Firestore...');
    await setDoc(overrideRef, overrides, { merge: true });
    console.log('Done!');
  } else {
    console.log('No new updates needed.');
  }

  process.exit(0);
}

run();
