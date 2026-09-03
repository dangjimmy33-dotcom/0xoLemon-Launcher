import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (rel) => fs.readFileSync(path.join(root, rel), 'utf8')
const assert = (condition, message) => { if (!condition) throw new Error(message) }

const social = read('social/SocialPrototype.tsx')
const socialCss = read('social/SocialPrototype.css')
const appCss = read('App.css')

assert(social.includes("--social-drawer-reserve"), 'social layer must publish the open drawer width to the global layout')
assert(social.includes("document.documentElement") && social.includes("style.setProperty('--social-drawer-reserve'"), 'social layer must update reserved width on the document root')
assert(appCss.includes('padding-right: var(--social-layout-reserve'), 'launcher shell must reserve the social rail/drawer width instead of being covered')
assert(appCss.includes('body.social-resizing .launcher-shell'), 'launcher layout transition must be disabled while the social drawer is being dragged')
assert(socialCss.includes('--social-layout-reserve'), 'social stylesheet must define the layout-reserve contract')
assert(!socialCss.includes('.social-global-layer.is-open::before'), 'drawer must not paint an overlay scrim over the workspace')

console.log('socialDrawerLayout.contract: PASS')
