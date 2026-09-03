# Social Ecosystem Canary

This runbook enables the production social backend for explicitly listed test
accounts without publishing a desktop release or opening the feature globally.

## Secret Preparation

1. Revoke every Hugging Face token that has appeared in chat, screenshots, shell
   history, or logs.
2. Create a fine-grained write token restricted to the public dataset
   `PROBBI/PROBBINE`.
3. Store it only as the Render secret `HF_SOCIAL_MEDIA_TOKEN`. Never place the
   value in `.env`, source control, Firestore, launcher settings, or React state.
4. Create an independent random `SOCIAL_ACCOUNT_HMAC_KEY` containing at least 32
   characters. Do not reuse an activation, Discord, Lua, or signing secret.

## Render Configuration

Set these variables on the existing backend service:

```text
SOCIAL_ENABLED=true
SOCIAL_CANARY_MODE=true
SOCIAL_CANARY_DISCORD_IDS=<comma-separated test Discord IDs>
SOCIAL_ACCOUNT_HMAC_KEY=<independent secret of at least 32 characters>
HF_SOCIAL_MEDIA_TOKEN=<fine-grained PROBBI/PROBBINE write token>
HF_SOCIAL_MEDIA_REPO=PROBBI/PROBBINE
HF_SOCIAL_MEDIA_BRANCH=main
SOCIAL_COVER_BATCH_SECONDS=600
```

Keep `SOCIAL_CANARY_MODE=true` throughout the canary. A non-allowlisted Discord
account must receive `SOCIAL_CANARY_ONLY` from `/social/bootstrap`.

## Dataset Bootstrap

Publish the contents of `backend-api/social/HF_DATASET_CARD.md` as the dataset
`README.md` before accepting the first cover. Confirm the repository is public,
the moderation policy is visible, and direct write access is limited to the
fine-grained Render token.

## Preflight

Run locally before changing Render:

```powershell
npm run build
node --test src/lib/*.test.mjs src/social/*.test.mjs
Push-Location backend-api
npm test
Pop-Location
Push-Location src-tauri
cargo check
cargo test --lib
Pop-Location
```

The Firestore emulator concurrency test is a separate gate and must not be
silently treated as passed when the emulator is unavailable.

## 24-Hour Canary Checklist

- Bootstrap succeeds for allowlisted accounts and rejects every other account.
- Profile edits survive launcher and Render restarts.
- A first cover appears locally immediately, remains pending, publishes in the
  next batch, and clears the matching pending hash only after the SSE event.
- A failed or rate-limited HF batch keeps the durable queue and retries without
  losing the previous public cover.
- Friend request, reciprocal accept, cancel, decline, remove, block, and unblock
  remain idempotent across two launcher windows.
- Playing presence wins over idle/online sessions, expires after the stale TTL,
  and `Appear offline` hides all active sessions.
- SSE reconnect resumes from `Last-Event-ID` after a Render restart.
- Leaderboard participation is opt-in; Global/Friends and Week/Month/All-time
  return consistent values without duplicate stat events.
- Logs contain no Discord bearer token, HF token, cover bytes, or raw account ID
  in HMAC-keyed storage paths.

Record failures and timestamps before changing the allowlist. Do not disable
canary mode or publish a launcher release until every item has remained stable
for at least 24 hours.
