import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const social = await readFile(new URL('../social/SocialPrototype.tsx', import.meta.url), 'utf8')
const socialCss = await readFile(new URL('../social/SocialPrototype.css', import.meta.url), 'utf8')
const helpCss = await readFile(new URL('../components/HelpSystem.css', import.meta.url), 'utf8')

assert.ok(social.includes('layerVisible'), 'social layer must have a visibility state separate from drawer open state')
assert.ok(social.includes("layer-visible"), 'social layer visibility must persist independently')
assert.ok(!social.includes('social-rail-close'), 'social rail must not add a separate X close button')
assert.match(social, /className="social-icon-button"[^>]*onClick=\{\(\) => setLayerVisible\(false\)\}/s, 'existing collapse chevron must hide the entire social layer')
assert.ok(social.includes('setLayerVisible(true)'), 'titlebar/social toggle must be able to restore the hidden social layer')
assert.ok(!socialCss.includes('.social-rail-close'), 'obsolete X close control styling must be removed')
assert.match(helpCss, /\.titlebar-help-button\s*\{[^}]*width:\s*24px\s*!important[^}]*height:\s*24px\s*!important/s, 'titlebar help button must use an exact square footprint')
assert.match(helpCss, /\.titlebar-help-button\s*\{[^}]*box-sizing:\s*border-box\s*!important/s, 'titlebar help size must include border and resist global button padding')
assert.match(helpCss, /\.titlebar-help-button\s*\{[^}]*border-radius:\s*9999px\s*!important/s, 'titlebar help button must remain a mathematically circular control')
assert.match(helpCss, /\.titlebar-help-button::before\s*\{[^}]*content:\s*'\?'/s, 'titlebar help circle must draw a stable question mark')

console.log('socialRailVisibilityAndHelpShape.contract: PASS')
