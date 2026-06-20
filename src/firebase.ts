import { initializeApp } from "firebase/app";
import { getFirestore } from "firebase/firestore";

const firebaseConfig = {
  apiKey: "AIzaSyAdqkMDJTjeVAfnwd8twlF2dvrKpWdNlUY",
  authDomain: "xolemon-b360e.firebaseapp.com",
  projectId: "xolemon-b360e",
  storageBucket: "xolemon-b360e.firebasestorage.app",
  messagingSenderId: "330469620392",
  appId: "1:330469620392:web:ad6f6e9288820f18ef209d",
  measurementId: "G-FZTWK4JCKG"
};

export const app = initializeApp(firebaseConfig);
export const db = getFirestore(app);
