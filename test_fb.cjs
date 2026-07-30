const { initializeApp } = require('firebase/app');
const { getFirestore, doc, getDoc } = require('firebase/firestore');
const app = initializeApp({
  apiKey: 'AIzaSyAdqkMDJTjeVAfnwd8twlF2dvrKpWdNlUY',
  projectId: 'xolemon-b360e'
});
const db = getFirestore(app);
getDoc(doc(db, 'gameCatalog', '007-first-light')).then(doc => {
  console.log(JSON.stringify(doc.data().availableVersions, null, 2));
  process.exit();
}).catch(e => { console.error(e); process.exit(1); });
