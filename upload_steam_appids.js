/**
 * upload_steam_appids.js
 * Chạy: node upload_steam_appids.js
 * 
 * Script này đọc steam_appids_mapping.json và upload lên Firestore
 * collection: config, document: steam_appids
 * Format: { [gameId]: appId }
 * Các game có appId = null sẽ bị bỏ qua
 */

const { initializeApp } = require('firebase/app')
const { getFirestore, doc, setDoc } = require('firebase/firestore')
const fs = require('fs')
const path = require('path')

const firebaseConfig = {
  apiKey: 'AIzaSyAdqkMDJTjeVAfnwd8twlF2dvrKpWdNlUY',
  authDomain: 'xolemon-b360e.firebaseapp.com',
  projectId: 'xolemon-b360e',
  storageBucket: 'xolemon-b360e.firebasestorage.app',
  messagingSenderId: '330469620392',
  appId: '1:330469620392:web:ad6f6e9288820f18ef209d'
}

const app = initializeApp(firebaseConfig)
const db = getFirestore(app)

async function upload() {
  const raw = fs.readFileSync(path.join(__dirname, 'steam_appids_mapping.json'), 'utf-8')
  const mapping = JSON.parse(raw)

  // Lọc bỏ các game chưa có appId (null)
  const filtered = Object.fromEntries(
    Object.entries(mapping).filter(([_, v]) => v !== null)
  )

  const count = Object.keys(filtered).length
  if (count === 0) {
    console.log('Không có appId nào để upload. Hãy điền vào steam_appids_mapping.json trước.')
    process.exit(0)
  }

  console.log(`Đang upload ${count} game appId lên Firestore...`)
  await setDoc(doc(db, 'config', 'steam_appids'), filtered, { merge: true })
  console.log('✅ Upload thành công!')
  Object.entries(filtered).forEach(([id, appid]) => console.log(`  ${id} -> ${appid}`))
  process.exit(0)
}

upload().catch(e => {
  console.error('❌ Lỗi:', e.message)
  process.exit(1)
})
