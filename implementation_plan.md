# Offline Activation Architecture

EA SPORTS FC 26 offline activation is owned by two production boundaries:

- `backend-api/activation` verifies Discord authorization, enforces the global
  rolling quota and account cooldown in Firestore, and talks to EA.
- `src-tauri/src/denuvo.rs` validates the registered game, verifies and installs
  the activation package, creates the ticket, applies the returned token, and
  maintains a resumable journal without secrets.

React only invokes the high-level Rust commands and renders
`OfflineActivationState`. It never receives a Discord access token, activation
ticket, EA token, arbitrary server URL, executable path, or configuration path.

The former local Python token server is retired and must not be used for release
or production activation.
