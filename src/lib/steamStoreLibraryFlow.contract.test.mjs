import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const app = await readFile(new URL('../App.tsx', import.meta.url), 'utf8')
const activeView = await readFile(new URL('../components/ActiveView.tsx', import.meta.url), 'utf8')
const library = await readFile(new URL('../components/library.tsx', import.meta.url), 'utf8')
const hook = await readFile(new URL('../hooks/useLauncherLibraryLayout.ts', import.meta.url), 'utf8')
const policy = await readFile(new URL('../themes/libraryPolicy.ts', import.meta.url), 'utf8')
const steamShell = await readFile(new URL('../themes/steam/SteamShell.tsx', import.meta.url), 'utf8')
const steamCss = await readFile(new URL('../themes/steam.css', import.meta.url), 'utf8')
const steamLibraryHome = await readFile(new URL('../themes/steam/SteamLibraryHome.tsx', import.meta.url), 'utf8')
const steamLibraryActivity = await readFile(new URL('../themes/steam/SteamLibraryActivity.tsx', import.meta.url), 'utf8')
const steamLibraryDetail = await readFile(new URL('../themes/steam/SteamLibraryDetail.tsx', import.meta.url), 'utf8')
const themeFixture = await readFile(new URL('../themes/ThemeFixture.tsx', import.meta.url), 'utf8')
const ownership = await readFile(new URL('./libraryOwnership.ts', import.meta.url), 'utf8')

test('Steam bottom utility bar belongs to Library only and remains viewport-persistent', () => {
  assert.match(steamShell, /activeTab === 'Library'/)
  assert.match(steamShell, /steam-bottom-bar/)
  assert.doesNotMatch(app, /SteamBottomBar/, 'Steam-only chrome must stay out of the shared App tree')
  assert.match(steamCss, /\.steam-bottom-bar\s*\{[\s\S]*?position:\s*fixed[\s\S]*?bottom:\s*0/)
})

test('Steam Store is acquisition-only: add to Library instead of installing or downloading', () => {
  assert.match(activeView, /uiTheme=\{uiTheme\}/)
  assert.match(library, /useLauncherLibraryLayout/)
  assert.doesNotMatch(library, /LAUNCHER_LIBRARY_KEY/, 'library membership must be persisted by Rust/AppData')
  assert.match(hook, /get_launcher_library_layout/)
  assert.match(hook, /save_launcher_library_layout/)
  assert.match(library, /Add to Library/)
  assert.match(library, /View in Library/)
  assert.match(library, /getThemeLibraryPresentation/)
  assert.match(policy, /acquisitionOnlyStore:\s*steam && viewMode === 'store'/)
  assert.match(library, /!acquisitionOnlyStore && effectiveMode !== 'steam'/)
})

test('Steam Library contains explicitly added games even before installation', () => {
  assert.match(app, /explicitLibraryGameIds:\s*launcherLibraryLayout\.libraryGameIds/)
  assert.match(app, /filterCatalogByOwnedGameIds\(catalog, ownedGameIds\)/)
  assert.match(app, /ownedGameIds\.has\(selectedGameId\)/)
  assert.match(ownership, /new Set\(explicitLibraryGameIds\)/)
  assert.match(library, /libraryGameIds/)
  assert.match(library, /libraryGameIds\.has\(game\.id\)/)
  assert.match(library, /onOpenLibrary/)
})

test('Library persistence failure restores the previous optimistic state', () => {
  assert.match(hook, /latestLayoutRef\.current === next/)
  assert.match(hook, /latestLayoutRef\.current = previous/)
  assert.match(hook, /setLayout\(previous\)/)
  assert.match(hook, /publishLayout\(previous\)/)
  assert.match(library, /libraryPersistFailed/)
})

test('Steam acquisition and owned navigation use distinct semantic actions', () => {
  assert.match(library, /data-library-state=\{inLauncherLibrary \? 'owned' : 'available'\}/)
  assert.match(steamCss, /--steam-acquire-start:\s*#75b022/)
  assert.match(steamCss, /steam-add-library-control\[data-library-state='available'\]/)
  assert.match(steamCss, /steam-add-library-control\[data-library-state='owned'\]/)
})

test('Steam Library collections are persisted and accept game drag-and-drop', () => {
  assert.match(library, /activeSteamCollectionId/)
  assert.match(library, /saveCollection/)
  assert.match(library, /application\/x-0xolemon-game-id/)
  assert.match(library, /addGameToSteamCollection/)
  assert.match(steamCss, /\.steam-library-custom-collection\.is-drop-target/)
})

test('Steam Library uses shelves rather than the Store grid and hover portal', () => {
  assert.match(library, /if \(showLibraryRail\) \{[\s\S]*?<SteamLibraryHome/)
  assert.match(steamLibraryHome, /What's New/)
  assert.match(steamLibraryHome, /Recent games/)
  assert.match(steamLibraryHome, /Play next/)
  assert.match(steamCss, /--steam-library-rail-width:\s*clamp\(290px, 21vw, 404px\)/)
  assert.match(steamCss, /\.steam-library-home\s*\{[\s\S]*?overflow-y:\s*auto/)
})

test('Steam game detail follows hero, action strip and information-row geometry', () => {
  assert.match(library, /steam-library-game-detail/)
  assert.match(library, /steam-detail-cover/)
  assert.match(library, /steam-detail-facts/)
  assert.match(library, /data-steam-action=/)
  assert.match(steamCss, /\.steam-library-game-detail \.store-action-dock\s*\{[\s\S]*?position:\s*relative/)
  assert.match(steamCss, /\.primary-control\[data-steam-action='install'\]/)
})

test('Steam Library detail uses activity layout while Store alone owns media and store panels', () => {
  assert.match(library, /if \(showLibraryRail\) \{[\s\S]*?<SteamLibraryDetail/)
  assert.match(library, /activeDetailTab === 'overview' && !showLibraryRail/)
  assert.match(steamLibraryDetail, /<SteamLibraryActivity/)
  assert.match(steamLibraryActivity, /steam-library-activity-layout/)
  assert.match(steamLibraryActivity, /steam-library-achievement-strip/)
  assert.doesNotMatch(steamLibraryDetail, /MediaRail|<video/)
  assert.doesNotMatch(steamLibraryActivity, /MediaRail|<video/)
  assert.match(steamCss, /\.steam-library-activity-layout\s*\{[\s\S]*?grid-template-columns:/)
})

test('Steam Library detail maps install state to real launcher actions', () => {
  assert.match(steamLibraryDetail, /installed[\s\S]*?label:\s*t\.library\.play[\s\S]*?action:\s*onPlay/)
  assert.match(steamLibraryDetail, /label:\s*t\.library\.chooseInstall[\s\S]*?action:\s*onInstall/)
  assert.match(steamLibraryDetail, /playing[\s\S]*?action:\s*onStop/)
  assert.match(steamLibraryDetail, /updateReady[\s\S]*?action:\s*onUpdate/)
  assert.doesNotMatch(steamLibraryDetail, /steam:\/\/|window\.open|MediaRail/)
})

test('Steam detail fixture uses the production one-row grid contract', () => {
  assert.match(themeFixture, /steam-library-dedicated-detail/)
  assert.match(steamCss, /\.game-detail-view\.steam-library-dedicated-detail\s*\{[\s\S]*?grid-template-rows:\s*minmax\(0, 1fr\)/)
  assert.match(steamCss, /\.game-detail-view\.steam-library-dedicated-detail > \.steam-library-detail-rail\s*\{[\s\S]*?grid-row:\s*1/)
  assert.match(steamCss, /\.game-detail-view\.steam-library-dedicated-detail > \.steam-library-detail-surface\s*\{[\s\S]*?grid-row:\s*1/)
})
