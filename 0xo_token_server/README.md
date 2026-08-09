# Retired offline activation prototype

This local Python prototype is no longer part of the launcher or production
deployment. Offline activation is implemented by the existing Render service in
`backend-api/activation` and by the high-level Rust commands in
`src-tauri/src/denuvo.rs`.

Do not restore local refresh tokens, EA credentials, activation tickets, machine
paths, or sample activation data in this directory. Production secrets belong in
the Render secret environment and rotating refresh tokens are encrypted in
Firestore.

Local automated tests use mocked Discord and EA clients. A real activation must
only be started deliberately from the launcher because it consumes one of the
five global slots.
