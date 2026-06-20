import { initializeApp } from "firebase/app";
import { getFirestore } from "firebase/firestore";

const firebaseConfig = {
  apiKey: "AIzaSyB2xXNcgWWbbUkUgoM4GXtkQZTjcFIU-io",
  authDomain: "xolemon.firebaseapp.com",
  projectId: "xolemon",
  storageBucket: "xolemon.firebasestorage.app",
  messagingSenderId: "174002283670",
  appId: "1:174002283670:web:07e033c0b9dc0a74b50eb7"
};

export const app = initializeApp(firebaseConfig);
export const db = getFirestore(app);
