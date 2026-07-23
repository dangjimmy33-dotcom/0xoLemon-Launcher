import { initializeApp } from 'firebase/app';
import { getFirestore, doc, setDoc } from 'firebase/firestore';

const firebaseConfig = {
  apiKey: "AIzaSyAdqkMDJTjeVAfnwd8twlF2dvrKpWdNlUY",
  authDomain: "xolemon-b360e.firebaseapp.com",
  projectId: "xolemon-b360e",
  storageBucket: "xolemon-b360e.firebasestorage.app",
  messagingSenderId: "527817093688",
  appId: "1:527817093688:web:a44ad0eb11c05a7b95c07c"
};

const app = initializeApp(firebaseConfig);
const db = getFirestore(app);

async function main() {
  const docRef = doc(db, 'config', 'version_tags');
  
  const initialData = {
    "007-first-light-v1.0.0": ["clean file game", "việt hóa"],
    "example-game-id-v2.0": ["cracked"],
    "example-game-id-v3.0": ["bypass hypervisor"]
  };
  
  console.log('Uploading version_tags to Firestore...');
  try {
    await setDoc(docRef, initialData);
    console.log('Upload success! Please refresh your Firebase Console.');
  } catch (error) {
    console.error('Failed to upload:', error.message);
  }
}

main().then(() => process.exit(0));
