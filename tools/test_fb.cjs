const { initializeApp, cert } = require('firebase-admin/app');
const { getFirestore } = require('firebase-admin/firestore');
const serviceAccount = require('../config/serviceAccountKey.json');
initializeApp({ credential: cert(serviceAccount) });
const db = getFirestore();
db.collection('gameDetails').doc('007-first-light').get().then(doc => {
  if (!doc.exists) { console.log('No such document!'); }
  else { console.log(JSON.stringify(doc.data().versions, null, 2)); }
  process.exit();
}).catch(err => { console.error(err); process.exit(1); });
