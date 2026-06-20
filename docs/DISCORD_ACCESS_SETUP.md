# Discord access setup

The launcher requires Discord OAuth before it unlocks the main interface. It requests only:

- `identify` for the signed-in user's display name and avatar.
- `guilds` to verify membership in guild `1492076309323714570`.

The launcher also derives the account creation timestamp from the Discord user snowflake and requires the account to be at least seven days old.

## Discord Developer Portal

1. Create or open the OAuth2 application used by 0xoLemon.
2. Add this exact redirect URL:

   `http://127.0.0.1:48176/discord/callback`

3. The launcher's default public Client ID is `1512105027270082651`.
4. Do not put the Discord Client Secret in the desktop launcher or repository.

## Local signed build

The launcher builds with the configured default Client ID. To override it for another Discord application, set the public Client ID in the same PowerShell session:

```powershell
$env:OXO_DISCORD_CLIENT_ID='YOUR_PUBLIC_DISCORD_CLIENT_ID'
npm run tauri build -- --target x86_64-pc-windows-msvc
```

The value is embedded at compile time. A runtime environment variable with the same name can override it for local testing.

## GitHub release build

The default Client ID works without additional GitHub configuration. To override it, create a GitHub Actions repository variable named `OXO_DISCORD_CLIENT_ID`; the release workflow already passes it to the Tauri build.

The workflow deliberately uses a repository variable, not a secret, because an OAuth Client ID is public. The Discord Client Secret must never be added to this desktop build.

## Policy behavior

- The launcher validates Discord online at every startup and every ten minutes while open.
- A missing/expired token, network verification failure, missing guild membership, or account younger than seven days locks the launcher.
- Access tokens are encrypted for the current Windows user with DPAPI.
- Install, update, repair, launch, uninstall, cloud-save mutation, and shortcut launch paths also require an authorized backend session.

This local gate deters normal redistribution but cannot prevent a determined attacker from patching the executable. Strong central revocation requires a server-side OAuth broker that stores the Client Secret and returns a short-lived signed launcher assertion.
