# 0xoLemon Backend API

The existing Render service is the only production backend for launcher
metadata and EA SPORTS FC 26 offline activation. Firestore is authoritative for
the rolling global quota, account cooldowns, idempotent requests, audit events,
and encrypted rotating EA credentials.

## Local checks

```powershell
cd E:\007Launcher\backend-api
npm install
npm test
node --check index.js
```

The test suite mocks Discord and EA. It never consumes a production activation.
The Firestore concurrency test runs only when `FIRESTORE_EMULATOR_HOST` is set.

## Activation API

- `GET /api/0xolemon/offline-activation/ea-sports-fc-26/status`
- `GET /api/0xolemon/offline-activation/ea-sports-fc-26/events`
- `GET /api/0xolemon/offline-activation/ea-sports-fc-26/me`
- `POST /api/0xolemon/offline-activation/ea-sports-fc-26/activate`
- `POST /api/0xolemon/offline-activation/ea-sports-fc-26/cancel`

`status` and `events` contain only public quota/package metadata. `me`,
`activate`, and `cancel` require the Discord bearer token and verify the
account, guild, role, and age directly against Discord.

`POST /api/:tenant/cache/clear` requires `X-Admin-Key`; it is not a public
maintenance endpoint.

## Required Render secrets

Use [`.env.example`](.env.example) as the schema. Real Firebase credentials,
the activation encryption/admin keys, EA credentials, package password, and
package metadata must exist only in Render's secret environment.

After deploying, enable a Firestore TTL policy for
`offlineActivationRequests.secretExpiresAt`. This removes encrypted recovery
tokens after the 96-hour cooldown. The numeric `tokenExpiresAtMs` remains the
authoritative application check even while Firestore TTL deletion is pending.

The package allowlist in `ACTIVATION_PACKAGE_FILES_JSON` must include every
file that Rust may install, with exact SHA-256 and byte size. A real activation
must be started manually from the launcher because it consumes one of the five
rolling ten-hour slots.
