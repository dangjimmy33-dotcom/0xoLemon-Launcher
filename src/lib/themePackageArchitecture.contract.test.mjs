import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8')
const [
  app,
  contracts,
  host,
  connectedHost,
  lightningShell,
  steamShell,
  xmclShell,
  layout,
  libraryHook,
  rustLayout,
  acl,
  fixture,
] = await Promise.all([
  read('../App.tsx'),
  read('../themes/contracts.ts'),
  read('../themes/ThemeShellHost.tsx'),
  read('../themes/ConnectedThemeShellHost.tsx'),
  read('../themes/lightning/LightningShell.tsx'),
  read('../themes/steam/SteamShell.tsx'),
  read('../themes/xmcl/XmclShell.tsx'),
  read('../components/layout.tsx'),
  read('../hooks/useLauncherLibraryLayout.ts'),
  read('../../src-tauri/src/library_layout.rs'),
  read('../../src-tauri/permissions/allow-all.json'),
  read('../themes/ThemeFixture.tsx'),
])

test('authenticated UI mounts exactly one dynamically selected theme package', () => {
  assert.match(app, /requestedShellTheme:\s*UiThemeId = hasLauncherAccess && !showIntro/)
  assert.match(app, /<ConnectedThemeShellHost/)
  assert.match(connectedHost, /<ThemeShellHost/)
  assert.match(connectedHost, /openFullProfile\(selfId\)/)
  assert.doesNotMatch(layout, /SteamPrimaryNav|XmclPrimaryNav/)
  assert.match(contracts, /loadShell:\s*\(\)\s*=>\s*import\('\.\/lightning\/LightningShell'\)/)
  assert.match(contracts, /loadShell:\s*\(\)\s*=>\s*import\('\.\/steam\/SteamShell'\)/)
  assert.match(contracts, /loadShell:\s*\(\)\s*=>\s*import\('\.\/xmcl\/XmclShell'\)/)
  assert.match(host, /Promise\.all\(\[shellPromise, framePromise\]\)/)
  assert.match(host, /loadedShells\.set/)
})

test('theme package failure is recoverable without changing the saved preference', () => {
  assert.match(host, /0xo-theme-package-error/)
  assert.match(host, /setActivePackage\(\{ theme: 'default'/)
  assert.match(app, /Default is active for this session; your saved preference was not changed/)
  assert.doesNotMatch(host, /saveLauncherPreferences/)
})

test('Lightning, Steam and XMCL shells own distinct navigation and capability surfaces', () => {
  assert.match(lightningShell, /lightning-primary-nav/)
  assert.match(lightningShell, /label: 'Nexus'/)
  assert.match(lightningShell, /label: 'Instant Gaming'/)
  assert.match(lightningShell, /label: 'Bypass'/)
  assert.match(lightningShell, /label: 'OnlineFix'/)
  assert.match(lightningShell, /tab: 'Tools'/)
  assert.match(steamShell, /steam-primary-nav/)
  assert.match(steamShell, /steam-bottom-bar/)
  assert.match(xmclShell, /xmcl-primary-nav/)
  assert.match(xmclShell, /visibleInstances\.map/)
  assert.match(xmclShell, /instanceGroups\.map/)
  assert.match(contracts, /motionProfile:\s*'lightning-cinematic'/)
  assert.match(contracts, /motionProfile:\s*'steam-desktop'/)
  assert.match(contracts, /motionProfile:\s*'xmcl-instance'/)
})

test('library personalization is atomically persisted by Rust and allowed by ACL', () => {
  assert.match(libraryHook, /get_launcher_library_layout/)
  assert.match(libraryHook, /save_launcher_library_layout/)
  assert.match(rustLayout, /atomic_write_path/)
  assert.match(rustLayout, /fn sanitize/)
  assert.match(acl, /get_launcher_library_layout/)
  assert.match(acl, /save_launcher_library_layout/)
})

test('development fixtures exercise all alternate packages with stable instance data', () => {
  assert.match(fixture, /FIXTURE_INSTANCES/)
  assert.match(fixture, /ThemeShellHost/)
  assert.match(fixture, /themeAccentMode:\s*'native'/)
})
