import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const titlebar = await readFile(new URL('../components/CustomTitleBar.tsx', import.meta.url), 'utf8')
const social = await readFile(new URL('../social/SocialPrototype.tsx', import.meta.url), 'utf8')
const css = await readFile(new URL('../App.css', import.meta.url), 'utf8')

assert.ok(titlebar.includes('titlebar-social-toggle'), 'titlebar must expose a persistent social expand/collapse control')
assert.ok(titlebar.includes('0xo-social-visibility-change'), 'titlebar toggle icon must track actual social layer visibility')
assert.ok(social.includes('0xo-social-visibility-change'), 'social layer must publish visibility changes back to the titlebar')
assert.match(css, /\.titlebar-social-toggle\s*\{[^}]*width:\s*30px[^}]*height:\s*28px/s, 'social titlebar toggle must be compact and aligned with titlebar utility controls')
assert.ok(titlebar.includes('CircleUserRound'), 'toggle must use the standard profile/user glyph requested for social visibility')
assert.ok(titlebar.includes('titlebar-social-state-dot'), 'profile toggle must still expose open/closed state without switching to panel-layout glyphs')
assert.ok(!titlebar.includes('PanelRightOpen') && !titlebar.includes('PanelRightClose'), 'panel-layout glyphs must stay removed from the profile/social toggle')

console.log('socialTitlebarToggle.contract: PASS')
