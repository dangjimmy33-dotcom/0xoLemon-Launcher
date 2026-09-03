import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (rel) => fs.readFileSync(path.join(root, rel), 'utf8')
const assert = (condition, message) => { if (!condition) throw new Error(message) }

const view = read('components/SettingsView.tsx')
const css = read('components/SettingsView.css')
const app = read('App.tsx')

assert(view.includes('type SettingsPaneId'), 'settings pane id type missing')
assert(view.includes('settings-pane-nav'), 'settings secondary navigation missing')
assert(view.includes('localStorage.getItem(SETTINGS_PANE_STORAGE_KEY)'), 'last settings pane must be restored')
assert(view.includes('data-active-pane={activePane}'), 'settings sections must expose active pane')
assert(css.includes('[data-settings-pane~="general"]'), 'settings pane transition must target the newly visible pane without remounting controls')
for (const pane of ['general', 'interface', 'games', 'components', 'storage', 'notifications', 'updates']) {
  assert(view.includes(`id: '${pane}'`), `settings pane metadata missing: ${pane}`)
}
assert(view.includes('data-settings-pane="general"'), 'general groups must be categorized')
assert(view.includes('data-settings-pane="interface"'), 'interface groups must be categorized')
assert(view.includes('data-settings-pane="games"'), 'games groups must be categorized')
assert(view.includes('data-settings-pane="components"'), 'large and optional components must have a dedicated pane')
assert(view.includes('data-settings-pane="storage"'), 'storage groups must be categorized')
assert(view.includes('data-settings-pane="notifications"'), 'notification groups must be categorized')
assert(view.includes('data-settings-pane="updates"'), 'update/about groups must be categorized')
assert(css.includes('.settings-pane-nav'), 'settings pane navigation styles missing')
assert(css.includes('.settings-sections[data-active-pane="general"]'), 'settings pane visibility rules missing')
assert(css.includes('@keyframes settings-pane-enter'), 'settings pane transition animation missing')
assert(view.includes("window.addEventListener('0xo-settings-pane'"), 'settings view must react to global pane navigation')
assert(app.includes("window.dispatchEvent(new CustomEvent('0xo-settings-pane'"), 'app navigation must route external settings links to the correct pane')
console.log('settingsPaneNavigation.contract: PASS')
