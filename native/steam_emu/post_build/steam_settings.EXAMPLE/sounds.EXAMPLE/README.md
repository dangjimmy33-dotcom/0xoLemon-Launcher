**This is currently available for Windows only**  

Place your overlay audio files here.  
At startup, any missing per-type file is automatically filled from the nearest generic fallback
in the chain below, so you only need to provide the files you want to customise.

**Fallback chains (most specific → least specific):**

Friend / lobby sounds:
```
overlay_invite_notification.wav       ← individual invite sound
overlay_chat_notification.wav         ← individual chat sound
overlay_auto_accept_notification.wav  ← individual auto-accept sound
overlay_lobby_join_request.wav        ← individual lobby-join-request sound
overlay_lobby_kicked.wav              ← individual kicked-from-lobby sound
overlay_friend_lobby.wav              ← individual friend-joined-lobby sound
overlay_lobby_status.wav              ← individual lobby/server status sound
  all of the above fall back to ↓
overlay_friend_notification.wav       ← generic friend/lobby fallback

overlay_lobby_join_accepted.wav       ← your join request was accepted
overlay_lobby_join_denied.wav         ← your join request was denied
  both fall back to ↓
overlay_lobby_join_response.wav       ← generic join-response fallback
  falls back to ↓
overlay_friend_notification.wav       ← generic friend/lobby fallback
```

Achievement sounds:
```
overlay_achievement_progress.wav      ← achievement progress updated (not yet unlocked)
  falls back to ↓
overlay_achievement_notification.wav  ← achievement unlocked (also the generic ach fallback)
```

