import { initializeApp } from 'firebase/app'
import { initializeFirestore, persistentLocalCache, persistentMultipleTabManager } from 'firebase/firestore'

// ─────────────────────────────────────────────────────────────
// 🟡 0xoLemon (xolemon-b360e) — Content DB
//    Collections: config/*, gameDetails/*
//    Dùng cho: game catalog, assets, version tags, app settings,
//              game tags, steam_appids, game stats
// ─────────────────────────────────────────────────────────────
const contentConfig = {
  apiKey: 'AIzaSyAdqkMDJTjeVAfnwd8twlF2dvrKpWdNlUY',
  authDomain: 'xolemon-b360e.firebaseapp.com',
  projectId: 'xolemon-b360e',
  storageBucket: 'xolemon-b360e.firebasestorage.app',
  messagingSenderId: '330469620392',
  appId: '1:330469620392:web:ad6f6e9288820f18ef209d',
  measurementId: 'G-FZTWK4JCKG',
}

// ─────────────────────────────────────────────────────────────
// 🔵 0xoLemon-1 (xolemon-1) — Social DB
//    Collections: chats/*, chat_meta/*, users/*
//    Dùng cho: game chat, remote control, pc_status/commands
// ─────────────────────────────────────────────────────────────
const socialConfig = {
  apiKey: 'AIzaSyBOeVOoaPMCX6gxnT7UTl_TlCBViBwDxPE',
  authDomain: 'xolemon-1.firebaseapp.com',
  projectId: 'xolemon-1',
  storageBucket: 'xolemon-1.firebasestorage.app',
  messagingSenderId: '813783362435',
  appId: '1:813783362435:web:1c1bbf3d56c082d9d7e4b6',
  measurementId: 'G-HSFJGDR1R3',
}

export const contentApp = initializeApp(contentConfig, 'content')
export const socialApp = initializeApp(socialConfig, 'social')

/** Content DB — game catalog, config, assets, game details */
export const contentDb = initializeFirestore(contentApp, {
  localCache: persistentLocalCache({ tabManager: persistentMultipleTabManager() }),
})

/** Social DB — chat, users, remote control commands */
export const socialDb = initializeFirestore(socialApp, {
  localCache: persistentLocalCache({ tabManager: persistentMultipleTabManager() }),
})

// Backward compat: các hooks cũ import `{ db }` vẫn trỏ đến contentDb
export const app = contentApp
export const db = contentDb
