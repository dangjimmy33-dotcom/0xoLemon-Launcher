import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const app = readFileSync(new URL('../App.tsx', import.meta.url), 'utf8')
const view = readFileSync(new URL('../themes/default/DefaultHomeView.tsx', import.meta.url), 'utf8')
const css = readFileSync(new URL('../themes/default/DefaultHomeView.css', import.meta.url), 'utf8')
const routeFrame = readFileSync(new URL('../themes/default/DefaultRouteFrame.tsx', import.meta.url), 'utf8')
const routeCss = readFileSync(new URL('../themes/default/DefaultRouteFrame.css', import.meta.url), 'utf8')
const types = readFileSync(new URL('../types.ts', import.meta.url), 'utf8')
const rust = readFileSync(new URL('../../src-tauri/src/home_wallpaper.rs', import.meta.url), 'utf8')
const handler = readFileSync(new URL('../../src-tauri/src/lib.rs', import.meta.url), 'utf8')
const permissions = readFileSync(new URL('../../src-tauri/permissions/allow-all.json', import.meta.url), 'utf8')
const desktopBootstrap = readFileSync(new URL('../desktop/bootstrap.tsx', import.meta.url), 'utf8')
const tauriConfig = JSON.parse(readFileSync(new URL('../../src-tauri/tauri.conf.json', import.meta.url), 'utf8'))

assert.ok(app.includes("renderedShellTheme === 'default' ? DefaultHomeView : HomeView"), 'only Default may use the iPadOS Home renderer')
assert.ok(types.includes("export type HomeWallpaperPreference"), 'wallpaper preference must be typed')
assert.ok(view.includes("updatePreference({ kind: 'pinned', assetId: wallpaperGame.id })"), 'featured games must be pinnable')
assert.ok(view.includes("invoke<HomeWallpaperAsset | null>('pick_home_wallpaper')"), 'custom photos must use the native importer')
assert.ok(view.includes("invoke<HomeWallpaperAsset>('get_home_wallpaper_asset'"), 'stored preferences must resolve by asset ID')
assert.ok(!view.includes('sourcePath'), 'frontend wallpaper preferences must not retain the source file path')
assert.ok(view.includes('new Image()'), 'wallpaper candidates must preload before being displayed')
assert.ok(view.includes('Reset to default'), 'wallpaper customization must provide a reset action')
assert.ok(view.includes('window.localStorage.removeItem(WALLPAPER_STORAGE_KEY)'), 'reset must remove the saved wallpaper preference')
assert.ok(view.includes('aria-haspopup="menu"'), 'wallpaper actions must live in an accessible compact menu')
for (const widget of ['User', 'Download', 'Online', 'Library', 'Update', 'Continue Playing']) {
  assert.ok(view.includes(widget), `Default Home widget missing: ${widget}`)
}
assert.ok(view.includes('onClick={onOpenDiscord}'), 'Default Home must preserve the Discord server action')
assert.ok(view.includes('onClick={onOpenDonate}'), 'Default Home must preserve the donate/support action')
assert.ok(view.includes('preferences.showDiscordCard'), 'Discord visibility must honor Home preferences')
assert.ok(view.includes('preferences.showDonateCard'), 'Donate visibility must honor Home preferences')
assert.ok(css.includes('grid-template-columns: repeat(3'), 'community actions must fit the compact context grid without adding another row')
assert.ok(css.includes('.default-home-wallpaper'), 'Default Home must own a full-bleed wallpaper layer')
assert.ok(css.includes('.default-wallpaper-customizer'), 'wallpaper controls must use the compact customizer')
assert.ok(css.includes('backdrop-filter: blur('), 'widgets must use the lock-screen glass treatment')
assert.ok(css.includes('@media (prefers-reduced-motion: reduce)'), 'Default Home must provide a static motion profile')
assert.ok(desktopBootstrap.includes("fixture === 'default-home'"), 'Default Home must expose a development-only visual QA fixture')
assert.equal(tauriConfig.app.security.assetProtocol.enable, true, 'Tauri asset protocol must be enabled for imported wallpapers')
assert.deepEqual(tauriConfig.app.security.assetProtocol.scope, ['$APPLOCALDATA/home-wallpapers/**'], 'asset protocol access must stay scoped to managed wallpapers')
assert.ok(routeFrame.includes('data-default-route={activeTab}'), 'Default routes must expose their animated frame state')
assert.ok(routeCss.includes('@keyframes default-route-forward-in'), 'Default routes must animate forward navigation')
assert.ok(routeCss.includes('@keyframes default-route-backward-in'), 'Default routes must animate backward navigation')
assert.ok(routeCss.includes('@media (prefers-reduced-motion: reduce)'), 'route transitions must respect reduced motion')

for (const command of ['pick_home_wallpaper', 'get_home_wallpaper_asset']) {
  assert.ok(handler.includes(`home_wallpaper::${command}`), `${command} must be registered`)
  assert.ok(permissions.includes(`"${command}"`), `${command} must be allowed by Tauri ACL`)
}
for (const guard of ['MAX_SOURCE_BYTES', 'max_image_width', 'max_alloc', 'SOURCE_REPARSE_REJECTED', 'HOME_WALLPAPER_FORMAT_REJECTED']) {
  assert.ok(rust.includes(guard), `wallpaper hardening guard missing: ${guard}`)
}
assert.ok(rust.includes('webp::Encoder::from_rgba'), 'custom images must be re-encoded as optimized assets')
assert.ok(rust.includes('Sha256::digest(&encoded)'), 'wallpaper asset IDs must be content addressed')

console.log('defaultHome.contract: PASS')
