import { initializeApp } from 'firebase/app'
import { getFirestore, doc, setDoc } from 'firebase/firestore'
import { readFileSync } from 'fs'
import { fileURLToPath } from 'url'
import { dirname, join } from 'path'
const __dirname = dirname(fileURLToPath(import.meta.url))
const firebaseConfig = { apiKey: 'AIzaSyAdqkMDJTjeVAfnwd8twlF2dvrKpWdNlUY', authDomain: 'xolemon-b360e.firebaseapp.com', projectId: 'xolemon-b360e', storageBucket: 'xolemon-b360e.firebasestorage.app', messagingSenderId: '330469620392', appId: '1:330469620392:web:ad6f6e9288820f18ef209d' }
const app = initializeApp(firebaseConfig)
const db = getFirestore(app)
const raw = readFileSync(join(__dirname, 'steam_appids_mapping.json'), 'utf-8').replace(/^\uFEFF/, '')
const mapping = JSON.parse(raw)
const filtered = Object.fromEntries(Object.entries(mapping).filter(([_, v]) => v !== null))
console.log('Uploading ' + Object.keys(filtered).length + ' entries to config/steam_appids...')
await setDoc(doc(db, 'config', 'steam_appids'), filtered)
Object.entries(filtered).forEach(([id, appid]) => console.log('  ' + id + ' -> ' + appid))
console.log('Done!')
process.exit(0)