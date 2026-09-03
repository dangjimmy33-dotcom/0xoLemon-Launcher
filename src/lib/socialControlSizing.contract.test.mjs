import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (rel) => fs.readFileSync(path.join(root, rel), 'utf8')
const assert = (condition, message) => { if (!condition) throw new Error(message) }

const social = read('social/SocialPrototype.tsx')
const css = read('social/SocialPrototype.css')

assert(
  social.includes('aria-label={t.social.collapseSocialPanel} title={t.social.collapseSocialPanel}') && social.includes('<ChevronRight size={20}'),
  'collapse control must use a clear directional chevron and visible tooltip'
)
assert(
  /\.social-icon-button\s*\{[\s\S]*?width:\s*36px;[\s\S]*?height:\s*36px;/.test(css),
  'collapse control must have a 36x36 visual button surface'
)
assert(
  /\.social-rail-add\s*\{[\s\S]*?width:\s*44px;[\s\S]*?min-height:\s*44px;/.test(css),
  'add friend rail action must expose a 44x44 target'
)
assert(
  social.includes('className="social-rail-add"') && social.includes('<UserPlus size={21}'),
  'add friend rail action must use a larger UserPlus icon'
)

console.log('socialControlSizing.contract: PASS')
