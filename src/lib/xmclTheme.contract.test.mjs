import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const themes = readFileSync(new URL('./uiThemes.ts', import.meta.url), 'utf8')
const app = readFileSync(new URL('../App.tsx', import.meta.url), 'utf8')
const layout = readFileSync(new URL('../components/layout.tsx', import.meta.url), 'utf8')
const contracts = readFileSync(new URL('../themes/contracts.ts', import.meta.url), 'utf8')
const host = readFileSync(new URL('../themes/ThemeShellHost.tsx', import.meta.url), 'utf8')
const xmclShell = readFileSync(new URL('../themes/xmcl/XmclShell.tsx', import.meta.url), 'utf8')
const settings = readFileSync(new URL('../components/SettingsView.tsx', import.meta.url), 'utf8')
const onboarding = readFileSync(new URL('../components/Onboarding.tsx', import.meta.url), 'utf8')
const picker = readFileSync(new URL('../themes/theme-picker.css', import.meta.url), 'utf8')
const social = readFileSync(new URL('../social/SocialPrototype.tsx', import.meta.url), 'utf8')
const socialCss = readFileSync(new URL('../social/SocialPrototype.css', import.meta.url), 'utf8')
const titlebar = readFileSync(new URL('../components/CustomTitleBar.tsx', import.meta.url), 'utf8')
const appCss = readFileSync(new URL('../App.css', import.meta.url), 'utf8')

assert.ok(themes.includes("'default' | 'lightning' | 'steam' | 'xmcl'"), 'theme registry must expose XMCL as a first-class built-in theme')
assert.ok(themes.includes("id: 'xmcl'"), 'XMCL profile must be registered')
assert.ok(!app.includes("import './themes/xmcl/index.css'"), 'XMCL stylesheet must not enter the default module graph')
assert.ok(!layout.includes('XmclPrimaryNav'), 'shared layout must not contain a hidden XMCL shell')
assert.ok(contracts.includes("import('./xmcl/XmclShell')"), 'XMCL shell must be dynamically imported')
assert.ok(host.includes('ThemeShellHost'), 'theme host must mount exactly one package shell')
assert.ok(xmclShell.includes('xmcl-shell-layout'), 'XMCL owns its structural shell class')
assert.ok(xmclShell.includes('visibleInstances.map'), 'XMCL rail must render real game instances')
assert.ok(xmclShell.includes('instanceGroups.map'), 'XMCL rail must render persisted instance groups')
assert.ok(app.includes('launcherLibraryLayout.collections.forEach'), 'XMCL groups must share persisted launcher collections')
assert.ok(app.includes("name: 'Favorites'"), 'XMCL rail must expose persisted launcher favorites as a group')
assert.ok(xmclShell.includes('onSelectGame(instance.gameId)'), 'XMCL instance selection must use launcher navigation actions')
assert.ok(settings.includes('Renderer riêng theo snapshot XMCL'), 'Settings theme picker must provide XMCL-specific localized copy')
assert.ok(onboarding.includes('Phong cách XMCL hiện đại'), 'Onboarding theme picker must name the XMCL experience')
assert.ok(picker.includes('.theme-preview-xmcl'), 'theme picker must preview XMCL geometry, not just a color swatch')

for (const path of [
  '../themes/xmcl/tokens.css',
  '../themes/xmcl/shell.css',
  '../themes/xmcl/surfaces.css',
  '../themes/xmcl/settings.css',
  '../themes/xmcl/motion.css',
]) {
  const content = readFileSync(new URL(path, import.meta.url), 'utf8')
  assert.ok(content.includes("html[data-ui-theme='xmcl']"), `${path} must be scoped to XMCL only`)
}

const shellCss = readFileSync(new URL('../themes/xmcl/shell.css', import.meta.url), 'utf8')
const surfacesCss = readFileSync(new URL('../themes/xmcl/surfaces.css', import.meta.url), 'utf8')
const motionCss = readFileSync(new URL('../themes/xmcl/motion.css', import.meta.url), 'utf8')
assert.match(shellCss, /\.xmcl-primary-nav[\s\S]*width:\s*80px/, 'XMCL navigation rail should use the compact 80px launcher rail from the reference UX')
assert.ok(!xmclShell.includes('<Sidebar'), 'default sidebar must not be mounted while XMCL is active')
assert.ok(surfacesCss.includes('backdrop-filter: blur'), 'XMCL package must theme dialogs/cards as frosted surfaces')
assert.ok(motionCss.includes('--xmcl-motion-duration'), 'XMCL package must define a coherent motion timing system')
assert.ok(motionCss.includes('.tab-enter'), 'page transitions must participate in the XMCL motion package')

assert.ok(titlebar.includes('CircleUserRound'), 'social visibility toggle must use a standard profile glyph')
assert.ok(!titlebar.includes('PanelRightOpen') && !titlebar.includes('PanelRightClose'), 'panel-layout glyphs must not be used for the profile/social toggle')
assert.ok(social.includes('<AnimatePresence initial={false}>'), 'social layer should stay mounted through exit animation instead of disappearing instantly')
assert.match(social, /duration:\s*0\.3[2-9]/, 'social reveal should use a calmer >=320ms transition')
assert.match(appCss, /padding-right\s+0\.3[2-9]s/, 'workspace reserve should animate at the same calmer pace as the social panel')
assert.ok(socialCss.includes('--social-motion-duration'), 'social layer should own a shared animation duration token')

console.log('xmclTheme.contract: PASS')
