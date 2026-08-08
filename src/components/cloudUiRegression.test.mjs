import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
const here = path.dirname(fileURLToPath(import.meta.url))
const root = path.resolve(here, '..')
const overview = fs.readFileSync(path.join(here, 'CloudSavesOverview.tsx'), 'utf8')
const settings = fs.readFileSync(path.join(here, 'CloudRedirectSettings.tsx'), 'utf8')
const css = fs.readFileSync(path.join(root, 'App.css'), 'utf8')
const crCss = fs.readFileSync(path.join(here, 'CloudRedirectSettings.css'), 'utf8')
// Native Cloud Save markup and CSS must agree on actual element types.
assert.match(css, /\.cloud-native-metrics\s*>\s*article/)
assert.match(css, /\.cloud-native-game-list\s*>\s*button/)
assert.match(overview, /className="cloud-native-title"/)
// CloudRedirect must poll Steam state without spawning the full provider/auth status path.
assert.match(settings, /cloud_redirect_engine_get_steam_state/)
assert.match(settings, /cloud_redirect_engine_close_steam/)
// The integrated view must use a restrained launcher-style flat status strip.
assert.match(crCss, /\.cr2-status-strip/)
assert.doesNotMatch(crCss, /box-shadow:\s*0\s+\d+px\s+\d+px/)
console.log('Cloud UI regression contract PASS')
