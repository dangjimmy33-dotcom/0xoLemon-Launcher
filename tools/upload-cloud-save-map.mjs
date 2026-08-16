/**
 * upload-cloud-save-map.mjs
 *
 * Scan tất cả file trong src-tauri/src/resources/cloud-save/games/*.json,
 * merge thành 1 CloudSaveMap document, rồi upload lên Firestore
 * xolemon-b360e → config/cloudSaveMap (contentDb).
 *
 * Cách dùng:
 *   node tools/upload-cloud-save-map.mjs
 *   node tools/upload-cloud-save-map.mjs --dry-run    (chỉ in ra, không upload)
 */

import { initializeApp } from 'firebase/app'
import { getFirestore, doc, setDoc, getDoc } from 'firebase/firestore'
import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'

// ─── Config ──────────────────────────────────────────────────────────────────

const firebaseConfig = {
  apiKey:            'AIzaSyAdqkMDJTjeVAfnwd8twlF2dvrKpWdNlUY',
  authDomain:        'xolemon-b360e.firebaseapp.com',
  projectId:         'xolemon-b360e',
  storageBucket:     'xolemon-b360e.firebasestorage.app',
  messagingSenderId: '330469620392',
  appId:             '1:330469620392:web:ad6f6e9288820f18ef209d',
}

const __dirname  = path.dirname(fileURLToPath(import.meta.url))
const GAMES_DIR  = path.resolve(__dirname, '../src-tauri/src/resources/cloud-save/games')
const DRY_RUN    = process.argv.includes('--dry-run')

// ─── Helpers ─────────────────────────────────────────────────────────────────

function readJson(filePath) {
  try {
    const raw = fs.readFileSync(filePath, 'utf8').replace(/^\uFEFF/, '')
    return JSON.parse(raw)
  } catch (e) {
    throw new Error(`Cannot parse ${path.basename(filePath)}: ${e.message}`)
  }
}

function bumpMapVersion(existing) {
  const today = new Date().toISOString().slice(0, 10).replace(/-/g, '.')
  const prefix = `${today}.`
  if (existing && existing.startsWith(prefix)) {
    const seq = parseInt(existing.slice(prefix.length), 10) || 0
    return `${prefix}${seq + 1}`
  }
  return `${prefix}1`
}

// ─── Validate per-game file ───────────────────────────────────────────────────

function validateGameFile(gameId, data) {
  if (!data || typeof data !== 'object') throw new Error(`${gameId}: not an object`)
  if (!Array.isArray(data.roots) || data.roots.length === 0)
    throw new Error(`${gameId}: "roots" is required and must be non-empty`)
  for (const root of data.roots) {
    if (!root.id) throw new Error(`${gameId}: every root must have an "id"`)
    if (!Array.isArray(root.candidates) || root.candidates.length === 0)
      throw new Error(`${gameId}/${root.id}: "candidates" is required and must be non-empty`)
    for (const c of root.candidates) {
      if (!c.base) throw new Error(`${gameId}/${root.id}: every candidate must have a "base"`)
    }
  }
}

// ─── Main ─────────────────────────────────────────────────────────────────────

async function main() {
  console.log('🗺️  Cloud Save Map — Upload Tool')
  console.log(`   Games dir : ${GAMES_DIR}`)
  if (DRY_RUN) console.log('   Mode      : DRY RUN (no upload)')
  console.log('')

  // 1. Scan per-game files
  if (!fs.existsSync(GAMES_DIR)) {
    console.error(`✗ Games directory not found: ${GAMES_DIR}`)
    process.exit(1)
  }

  const gameFiles = fs.readdirSync(GAMES_DIR)
    .filter(f => f.endsWith('.json'))
    .sort()

  if (gameFiles.length === 0) {
    console.error('✗ No game JSON files found in games/')
    process.exit(1)
  }

  console.log(`📂 Found ${gameFiles.length} game file(s):`)
  const games = {}
  let hasErrors = false

  for (const file of gameFiles) {
    const gameId = file.replace(/\.json$/, '')
    const filePath = path.join(GAMES_DIR, file)
    try {
      const data = readJson(filePath)
      validateGameFile(gameId, data)
      games[gameId] = data
      console.log(`   ✓ ${gameId}  (${data.roots.length} root${data.roots.length > 1 ? 's' : ''})`)
    } catch (e) {
      console.log(`   ✗ ${gameId}: ${e.message}`)
      hasErrors = true
    }
  }

  if (hasErrors) {
    console.error('\n✗ Validation failed — fix errors above before uploading.')
    process.exit(1)
  }

  // 2. Fetch existing mapVersion from Firestore for bump
  let existingVersion = null
  if (!DRY_RUN) {
    const fb  = initializeApp(firebaseConfig)
    const db  = getFirestore(fb)
    const ref = doc(db, 'config', 'cloudSaveMap')
    try {
      const snap = await getDoc(ref)
      if (snap.exists()) existingVersion = snap.data()?.mapVersion ?? null
    } catch (_) { /* first upload */ }
  }

  const mapVersion = bumpMapVersion(existingVersion)
  const now        = new Date().toISOString()

  // 3. Build final CloudSaveMap document
  const document = {
    schemaVersion:          1,
    mapVersion,
    platform:               'windows',
    minimumLauncherVersion: '0.1.1',
    publishedAt:            now,
    expiresAt:              '2099-01-01T00:00:00Z',
    defaults: {
      syncMode:            'automatic',
      syncBeforeLaunch:    true,
      syncAfterExit:       true,
      followReparsePoints: false,
      excludeWins:         true,
      limits: {
        maxFiles:       10000,
        maxTotalBytes:  5368709120,
        maxFileBytes:   2147483648,
      },
      stability: {
        settleTimeMs:   2000,
        pollIntervalMs: 500,
        maxWaitMs:      30000,
      },
      retention: {
        recent:       10,
        dailyDays:    7,
        weeklyWeeks:  4,
        conflictDays: 90,
      },
      migration: {
        legacyRetentionDays: 30,
      },
    },
    games,
  }

  console.log(`\n📦 Map version   : ${mapVersion}`)
  console.log(`   Games count  : ${Object.keys(games).length}`)
  console.log(`   Published at : ${now}`)

  if (DRY_RUN) {
    console.log('\n[DRY RUN] Document preview:')
    console.log(JSON.stringify(document, null, 2))
    return
  }

  // 4. Upload to Firestore
  console.log('\n⬆️  Uploading to Firestore xolemon-b360e → config/cloudSaveMap ...')
  const fb  = initializeApp(firebaseConfig, 'upload-run')
  const db  = getFirestore(fb)
  const ref = doc(db, 'config', 'cloudSaveMap')
  await setDoc(ref, document)
  console.log('✅ Upload complete! Launcher sẽ nhận map mới trong vài giây.')
}

main().catch(err => {
  console.error('✗ Fatal:', err.message ?? err)
  process.exit(1)
})
