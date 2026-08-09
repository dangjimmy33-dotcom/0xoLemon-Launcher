const assert = require('node:assert/strict');
const test = require('node:test');

const { deleteApp, initializeApp } = require('firebase-admin/app');
const { getFirestore } = require('firebase-admin/firestore');
const {
  ActivationQuotaService,
  FirestoreActivationStore
} = require('../activation/quota-service');

const emulatorAvailable = Boolean(process.env.FIRESTORE_EMULATOR_HOST);

test('Firestore transaction admits exactly five concurrent reservations', {
  skip: !emulatorAvailable
}, async () => {
  const suffix = `${process.pid}-${Date.now()}`;
  const gameId = `ea-sports-fc-26-emulator-${suffix}`;
  const app = initializeApp({ projectId: 'demo-offline-activation' }, `activation-test-${suffix}`);
  const service = new ActivationQuotaService(
    new FirestoreActivationStore(getFirestore(app)),
    {
      gameId,
      capacity: 5,
      windowMs: 10 * 60 * 60 * 1000,
      cooldownMs: 96 * 60 * 60 * 1000,
      reservationMs: 3 * 60 * 1000,
      accountRateWindowMs: 15 * 60 * 1000,
      accountRateMax: 8
    },
    () => Date.UTC(2026, 7, 9, 0, 0, 0)
  );

  try {
    const results = await Promise.all(Array.from({ length: 6 }, (_, index) => {
      const ordinal = index + 1;
      return service.reserve({
        requestId: `10000000-0000-4000-8000-${String(ordinal).padStart(12, '0')}`,
        accountKey: `account-${suffix}-${ordinal}`,
        ticketHash: `ticket-${suffix}-${ordinal}`,
        launcherVersion: '2.0.42'
      }).then(() => 'reserved').catch((error) => error.code);
    }));
    assert.equal(results.filter((result) => result === 'reserved').length, 5);
    assert.equal(results.filter((result) => result === 'NO_GLOBAL_SLOT').length, 1);
  } finally {
    await deleteApp(app);
  }
});
