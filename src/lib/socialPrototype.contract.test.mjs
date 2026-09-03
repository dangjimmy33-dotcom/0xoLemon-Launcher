import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (rel) => fs.readFileSync(path.join(root, rel), 'utf8')
const exists = (rel) => fs.existsSync(path.join(root, rel))
const assert = (condition, message) => { if (!condition) throw new Error(message) }

assert(exists('social/socialApi.ts'), 'missing production social API module')
assert(exists('social/SocialPrototype.tsx'), 'missing SocialPrototype component')
assert(exists('social/SocialPrototype.css'), 'missing SocialPrototype stylesheet')

const api = read('social/socialApi.ts')
const social = read('social/SocialPrototype.tsx')
const css = read('social/SocialPrototype.css')
const app = read('App.tsx')
const types = read('types.ts')
const layout = read('components/layout.tsx')
const help = read('lib/helpRegistry.ts')
const titlebar = read('components/CustomTitleBar.tsx')
const en = read('i18n/en-US.ts')
const vi = read('i18n/vi-VN.ts')

assert(types.includes("| 'Social'"), 'Social TabId not registered')
assert(layout.includes("['Social'"), 'Social sidebar item missing')
assert(app.includes("activeTab === 'Social'"), 'App does not render Social hub')
assert(app.includes('SocialPrototypeProvider'), 'App missing social provider')
assert(app.includes('SocialPrototypeLayer'), 'App missing global social layer')
assert(titlebar.includes('onToggleSocial'), 'titlebar social toggle missing')
assert(app.includes('onToggleSocial='), 'App does not wire titlebar social toggle')
assert(social.includes('t.social.playing.toUpperCase()') && en.includes('playing:') && vi.includes('playing:'), 'drawer missing localized Playing section')
assert(social.includes('t.social.online.toUpperCase()') && en.includes('online:') && vi.includes('online:'), 'drawer missing localized Online section')
assert(social.includes('function Leaderboard') && social.includes('t.social.leaderboards'), 'social hub missing leaderboards')
assert(social.includes('t.social.addFriend'), 'social hub missing localized Add Friend action')
assert(social.includes('social-profile-popout'), 'profile popout missing')
assert(social.includes('social-full-profile'), 'full profile view missing')
assert(css.includes('.social-drawer'), 'social drawer styling missing')
assert(css.includes('.social-rail'), 'social rail styling missing')
assert(css.includes('@media (max-width: 1449px)'), 'responsive drawer breakpoint missing')
assert(help.includes("'Social': 'social'"), 'help registry missing Social')
assert(api.includes("invoke<SocialBootstrapDto>('get_social_bootstrap'"), 'social UI must bootstrap through a Rust command')
assert(api.includes("listen<SocialServerEvent>('social://event'"), 'social UI must receive backend SSE through Tauri events')
assert(!social.includes('createPrototypeMembers'), 'production social UI must not seed prototype members')
assert(!/firebase\/firestore|from ['"]firebase/.test(api + social), 'launcher social code must not access Firestore directly')

console.log('socialPrototype.contract: PASS')
