import fs from 'node:fs'
import path from 'node:path'
import assert from 'node:assert/strict'

const srcRoot = path.resolve('src')
const read = (relativePath) => fs.readFileSync(path.join(srcRoot, relativePath), 'utf8')

const app = read('App.tsx')
const appCss = read('App.css')
const layout = read('components/layout.tsx')
const social = read('social/SocialPrototype.tsx')
const premium = read('premium.css')
const reveal = read('hooks/useScrollReveal.ts')

assert.ok(
  app.includes('activeTab === "What\'s New!" ? \' whats-new-workspace\''),
  "What's New must use a dedicated outer workspace state",
)
assert.ok(
  app.includes('activeTab === "What\'s New!" ? \'whats-new-tab-content\''),
  "What's New must establish a definite-height tab wrapper",
)
assert.match(
  appCss,
  /\.workspace\.whats-new-workspace\s*\{[\s\S]*?overflow:\s*hidden/,
  "The app workspace must not steal What's New scroll events",
)
assert.match(
  appCss,
  /\.tab-content\.whats-new-tab-content\s*\{[\s\S]*?min-height:\s*0;[\s\S]*?height:\s*100%;[\s\S]*?overflow:\s*hidden/,
  "What's New wrapper must give its Motion scroll container a definite height",
)
assert.ok(!layout.includes("['Cache', t.nav.cache, Database]"), 'Cache must not be a primary sidebar item')
assert.ok(social.includes("style.setProperty('--social-layout-reserve'"), 'Social drawer must publish the full shell reserve')
assert.ok(social.includes("style.removeProperty('--social-layout-reserve'"), 'Social drawer reserve must be cleaned up')
assert.ok(!/selector\s*=.*\.settings-group/.test(reveal), 'Hidden Settings groups must not depend on IntersectionObserver reveal state')
assert.match(
  premium,
  /\.titlebar-discord-user span\s*\{[\s\S]*?flex:\s*1 1 auto;[\s\S]*?font-family:/,
  'Titlebar profile name must have a stable flex slot and UI-safe font',
)

console.log('UI shell regression contract PASS')
