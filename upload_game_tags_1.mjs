/**
 * upload_game_tags_1.mjs  [0xoLemon-1 / xolemon-1]
 * Bản copy của upload_game_tags.mjs nhưng trỏ vào Firebase project 0xoLemon-1.
 * Đọc file game-tags.json ở local và gộp lên Firestore (0xoLemon-1).
 * ưu tiên GIỬ NGUYÊN các tag đã sửa trực tiếp trên Firestore (Remote wins).
 * Chỉ thêm các game mới có ở local mà trên Firestore chưa có.
 */

import { initializeApp } from 'firebase/app';
import { getFirestore, doc, getDoc, setDoc } from 'firebase/firestore';
import fs from 'fs';

const firebaseConfig = {
  apiKey: "AIzaSyBOeVOoaPMCX6gxnT7UTl_TlCBViBwDxPE",
  authDomain: "xolemon-1.firebaseapp.com",
  projectId: "xolemon-1",
  storageBucket: "xolemon-1.firebasestorage.app",
  messagingSenderId: "813783362435",
  appId: "1:813783362435:web:1c1bbf3d56c082d9d7e4b6"
};

const app = initializeApp(firebaseConfig);
const db = getFirestore(app);

async function main() {
  console.log('Đang đọc src-tauri/game-tags.json...');
  const localData = JSON.parse(fs.readFileSync('src-tauri/game-tags.json', 'utf8'));
  
  const docRef = doc(db, 'config', 'gameTags');
  console.log('Đang fetch dữ liệu gameTags từ Firestore...');
  const snap = await getDoc(docRef);
  const remoteData = snap.exists() ? snap.data() : { games: {}, definitions: {} };
  
  // Gộp definitions (các định nghĩa tag như 'denuvo', 'online') 
  // Ưu tiên local cho definitions để luôn có định nghĩa mới nhất
  const mergedDefinitions = { ...(remoteData.definitions || {}), ...(localData.definitions || {}) };
  
  // Gộp games
  // Ưu tiên REMOTE: Những gì bạn đã sửa trên Firestore (remote) sẽ ghi đè local
  // Nhờ đó, nếu local có thêm game mới (persona-3-reload), nó sẽ được thêm vào
  // nhưng các game cũ đã config tay trên web sẽ không bị mất.
  const mergedGames = { ...(localData.games || {}), ...(remoteData.games || {}) };
  
  const finalData = {
    schemaVersion: localData.schemaVersion || 1,
    definitions: mergedDefinitions,
    games: mergedGames
  };
  
  console.log('\nĐang upload dữ liệu đã gộp lên Firestore...');
  await setDoc(docRef, finalData); // setDoc không có {merge:true} vì ta đã tự gộp chuẩn xác 100% bằng code rồi
  console.log(`✅ Upload thành công! Tổng cộng có ${Object.keys(mergedGames).length} game tags.`);
  process.exit(0);
}

main().catch(e => {
  console.error(e);
  process.exit(1);
});
