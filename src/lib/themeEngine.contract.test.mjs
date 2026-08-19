import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const preferences = await readFile(new URL('./preferences.ts', import.meta.url), 'utf8')
const theme = await readFile(new URL('./theme.ts', import.meta.url), 'utf8')
const css = await readFile(new URL('../index.css', import.meta.url), 'utf8')
const appCss = await readFile(new URL('../App.css', import.meta.url), 'utf8')
const app = await readFile(new URL('../App.tsx', import.meta.url), 'utf8')
const settings = await readFile(new URL('../components/SettingsView.tsx', import.meta.url), 'utf8')
const en = await readFile(new URL('../i18n/en-US.ts', import.meta.url), 'utf8')
const vi = await readFile(new URL('../i18n/vi-VN.ts', import.meta.url), 'utf8')

for (const key of ['themeIntensity', 'themeContrast', 'dynamicTheme', 'dynamicThemeSpeed']) {
  assert.ok(preferences.includes(key), `preferences must persist ${key}`)
}

assert.ok(theme.includes('--theme-runtime-hue'), 'theme engine must expose a runtime hue')
assert.ok(theme.includes('--theme-card-bg'), 'theme engine must derive semantic card surfaces')
assert.ok(theme.includes('--theme-elevated-bg'), 'theme engine must derive elevated surfaces')
assert.ok(theme.includes('data-theme-dynamic'), 'theme engine must expose dynamic theme state')
assert.ok(theme.includes('data-theme-motion'), 'theme engine must expose motion policy')
assert.ok(css.includes('@property --theme-runtime-hue'), 'runtime hue must be a typed custom property')
assert.ok(css.includes('@keyframes launcher-hue-cycle'), 'dynamic hue must use smooth CSS interpolation')
assert.ok(css.includes('prefers-reduced-motion: reduce'), 'dynamic theme must respect reduced motion')

assert.match(appCss, /\.workspace\s*\{[\s\S]*?background:\s*var\(--launcher-page-bg/, 'workspace must use the semantic page background')
assert.match(appCss, /\.settings-group\s*\{[\s\S]*?background:\s*var\(--theme-card-bg/, 'settings cards must use semantic themed surfaces')
assert.ok(!appCss.includes('Sidebar stays graphite'), 'sidebar must no longer be intentionally locked to graphite')
assert.match(appCss, /\.titlebar-label-primary\s*\{[\s\S]*?var\(--theme-accent/, 'titlebar brand color must follow launcher theme')
assert.ok(theme.includes('--launcher-frame-bg'), 'theme engine must expose one shared frame surface for titlebar and sidebar')
assert.match(appCss, /\.custom-titlebar\s*\{[\s\S]*?background:\s*var\(--launcher-frame-bg/, 'titlebar must use the same frame surface as the sidebar so the top-left seam disappears')

for (const key of ['themeIntensity', 'themeContrast', 'dynamicTheme', 'dynamicThemeSpeed']) {
  assert.ok(settings.includes(key), `Color Studio must expose ${key}`)
}
for (const token of ['themeSaturation', 'themeIntensity', 'themeContrast', 'dynamicTheme', 'dynamicThemeSpeed']) {
  assert.ok(en.includes(token), `English i18n must contain ${token}`)
  assert.ok(vi.includes(token), `Vietnamese i18n must contain ${token}`)
}

console.log('adaptive theme engine contract PASS')

// Regression: palette must tint the shell/content, ambient motion must obey Dynamic Theme,
// and the rounded workspace junction must reveal a semantic chrome/sidebar surface.
const premiumCss = await readFile(new URL('../premium.css', import.meta.url), 'utf8')
const whatsNew = await readFile(new URL('../components/WhatsNewView.tsx', import.meta.url), 'utf8')

assert.match(appCss, /\.launcher-shell\s*\{[\s\S]*?background:\s*var\(--launcher-frame-bg/, 'shell behind the rounded workspace corner must use the shared frame surface')
assert.match(appCss, /html\[data-theme-dynamic='true'\][\s\S]*?\.launcher-shell::before[\s\S]*?animation:\s*ambient-drift/, 'ambient drift must only animate while Dynamic Theme is enabled')
const baseAmbientBefore = appCss.match(/\.launcher-shell::before\s*\{([\s\S]*?)\n\}/)?.[1] ?? ''
assert.ok(!baseAmbientBefore.includes('animation: ambient-drift'), 'ambient drift must not run unconditionally')
assert.match(premiumCss, /\.premium-workspace\s*\{[\s\S]*?var\(--launcher-page-bg/, 'premium workspace must use semantic page surfaces instead of hard-coded graphite')
assert.match(premiumCss, /\.premium-shell\s*>\s*\.sidebar\s*\{[\s\S]*?background:\s*var\(--launcher-frame-bg/, 'premium sidebar must share the titlebar frame surface at the junction')
assert.ok(!app.includes('sidebar-workspace-junction'), 'launcher shell must not render the old mirrored junction helper')
assert.match(appCss, /\.panel\s*\{[\s\S]*?background:\s*var\(--theme-card-bg/, 'common panels must follow the adaptive card palette')
assert.match(appCss, /\.translation-catalog-card\s*\{[\s\S]*?background:\s*var\(--theme-card-bg/, 'translation cards must follow the adaptive card palette')
assert.ok(!whatsNew.includes('#4da4ff'), "What's New must not keep the old fixed blue accent")
assert.ok(!whatsNew.includes('rgba(77, 164, 255'), "What's New must not keep old fixed-blue RGBA surfaces")

const homeCss = await readFile(new URL('../home-view.css', import.meta.url), 'utf8')
const cloudRedirectCss = await readFile(new URL('../components/CloudRedirectSettings.css', import.meta.url), 'utf8')
assert.match(homeCss, /--hv-surface:\s*var\(--theme-card-bg-soft\)/, 'Home local surfaces must inherit the global adaptive palette')
assert.match(cloudRedirectCss, /--cr2-surface:var\(--theme-card-bg\)/, 'Cloud Redirect local surfaces must inherit the global adaptive palette')


// Regression: the sidebar/workspace junction must be a single continuous curve,
// and startup/access/help/Lua Shop overlays must inherit Color Studio surfaces.
const helpCss = await readFile(new URL('../components/HelpSystem.css', import.meta.url), 'utf8')
const luaShopCss = await readFile(new URL('../components/LuaShop.css', import.meta.url), 'utf8')

const premiumSidebarBlock = premiumCss.match(/\.premium-shell\s*>\s*\.sidebar\s*\{([\s\S]*?)\n\}/)?.[1] ?? ''
assert.ok(!premiumSidebarBlock.includes('border-top-right-radius'), 'ChatGPT-style junction keeps the sidebar square; the content pane owns the curve')
assert.match(premiumSidebarBlock, /background:\s*var\(--launcher-frame-bg/, 'sidebar top surface must be identical to the titlebar frame surface')
assert.ok(!appCss.includes('.sidebar::before'), 'sidebar must not paint a second tint layer that reintroduces a horizontal seam below the titlebar')
assert.ok(!appCss.includes('.sidebar-workspace-junction'), 'the old helper must be removed so it cannot draw a second/mirrored curve')
const workspaceBlock = appCss.match(/\.workspace\s*\{([\s\S]*?)\n\}/)?.[1] ?? ''
const workspaceClipBlock = appCss.match(/\.workspace-corner-clip\s*\{([\s\S]*?)\n\}/)?.[1] ?? ''
const premiumWorkspaceBlock = premiumCss.match(/\.premium-workspace\s*\{([\s\S]*?)\n\}/)?.[1] ?? ''
assert.match(app, /<div className=\"workspace-corner-clip\">[\s\S]*?<section[\s\S]*?className=\{`workspace premium-workspace/, 'workspace must be wrapped by a dedicated rounded clip layer')
assert.match(workspaceClipBlock, /border-top-left-radius:\s*var\(--launcher-content-radius/, 'dedicated clip layer must own the visible top-left curve')
assert.match(workspaceClipBlock, /overflow:\s*hidden/, 'dedicated clip layer must hard-clip the workspace and reveal the shell beneath the rounded corner')
assert.match(workspaceClipBlock, /background:\s*transparent/, 'rounded corner wrapper must be transparent so the frame layer underneath is exposed')
assert.ok(!workspaceBlock.includes('border-top-left-radius'), 'scrolling workspace must not own the corner radius; clipping belongs to the stable wrapper')
assert.ok(!workspaceBlock.includes('clip-path'), 'scrolling workspace must not rely on compositor-dependent clip-path geometry')
assert.ok(!premiumWorkspaceBlock.includes('border-top-left-radius'), 'premium workspace must not draw a second curve')
assert.match(appCss, /\.intro-screen\s*\{[\s\S]*?background:\s*var\(--launcher-page-bg/, 'cinematic intro must inherit the launcher page palette')
assert.match(appCss, /\.intro-ring-arc\s*\{[\s\S]*?stroke:\s*var\(--theme-accent-strong/, 'intro progress ring must follow the active accent')
assert.match(premiumCss, /\.onboarding-card\s*\{[\s\S]*?var\(--theme-modal-bg/, 'first-run onboarding card must inherit themed surfaces')
assert.match(premiumCss, /\.discord-access-gate\s*\{[\s\S]*?var\(--theme-overlay-bg/, 'Discord access backdrop must inherit the launcher palette')
assert.match(premiumCss, /\.discord-access-card\s*\{[\s\S]*?var\(--theme-modal-bg/, 'Discord access card must inherit themed surfaces')
assert.match(helpCss, /\.help-center\s*\{[\s\S]*?var\(--theme-card-bg/, 'Help Center shell must inherit themed surfaces')
assert.match(helpCss, /\.help-center-mark\s*\{[\s\S]*?var\(--theme-accent/, 'Help Center accents must follow Color Studio')
assert.match(luaShopCss, /\.lua-manager-dialog\s*\{[\s\S]*?var\(--theme-card-bg/, 'Lua manager dialog must inherit themed surfaces')
assert.match(luaShopCss, /\.lua-source-picker\s*\{[\s\S]*?var\(--theme-card-bg/, 'Lua source picker must inherit themed surfaces')
assert.match(luaShopCss, /\.lua-source-option-card\.is-selected\s*\{[\s\S]*?var\(--theme-accent/, 'Lua source selection highlight must follow Color Studio')

// Regression: restart-Steam confirmation must inherit Color Studio instead of fixed graphite/green.
const confirmDialog = await readFile(new URL('../components/ConfirmDialog.tsx', import.meta.url), 'utf8')
const libraryView = await readFile(new URL('../components/library.tsx', import.meta.url), 'utf8')
const luaShopView = await readFile(new URL('../components/LuaShop.tsx', import.meta.url), 'utf8')
assert.match(appCss, /\.confirm-dialog\s*\{[\s\S]*?background:\s*var\(--theme-modal-bg/, 'shared confirmation dialog surface must inherit Color Studio')
assert.match(appCss, /\.confirm-dialog-backdrop\s*\{[\s\S]*?background:\s*var\(--theme-overlay-bg/, 'confirmation backdrop must inherit the themed overlay token')
assert.match(appCss, /\.confirm-dialog-info\s*\{[\s\S]*?var\(--theme-accent/, 'info confirmation action must use the active theme accent instead of fixed blue/green')
assert.match(appCss, /\.confirm-dialog-icon--info\s*\{[\s\S]*?var\(--theme-accent/, 'info confirmation icon must use the active theme accent')
assert.ok(!confirmDialog.includes("background: rgba(15, 16, 19"), 'shared ConfirmDialog must not hard-code a graphite modal surface')
assert.ok(libraryView.includes("background: 'var(--theme-control-bg)'"), 'library restart options panel must inherit themed control surfaces')
assert.ok(luaShopView.includes("background: 'var(--theme-modal-bg)'"), 'Lua Shop confirmation surface must inherit themed modal surfaces')
assert.ok(luaShopView.includes("background: 'var(--theme-control-bg)'"), 'Lua Shop restart options panel must inherit themed control surfaces')

// Regression: first-run guided tour must inherit Color Studio instead of fixed cyan/graphite.
assert.match(appCss, /\.guided-tour-dim\s*\{[\s\S]*?var\(--theme-overlay-bg/, 'guided tour backdrop must use themed overlay surface')
assert.match(appCss, /\.guided-tour-card\s*\{[\s\S]*?var\(--theme-modal-bg/, 'guided tour card must use themed modal surface')
assert.match(appCss, /\.guided-tour-icon\s*\{[\s\S]*?var\(--theme-accent-strong/, 'guided tour icon must follow the active theme accent')
assert.match(appCss, /\.guided-tour-dots i\.active\s*\{[\s\S]*?var\(--theme-accent-strong/, 'guided tour progress must follow Color Studio')
assert.match(appCss, /\.guided-tour-primary\s*\{[\s\S]*?var\(--theme-accent/, 'guided tour primary action must follow Color Studio')
