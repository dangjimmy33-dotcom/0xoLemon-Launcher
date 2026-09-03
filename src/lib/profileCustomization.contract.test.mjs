import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (rel) => fs.readFileSync(path.join(root, rel), 'utf8')
const exists = (rel) => fs.existsSync(path.join(root, rel))
const assert = (condition, message) => { if (!condition) throw new Error(message) }

assert(exists('social/socialProfileStorage.ts'), 'missing local social profile storage module')
const storage = exists('social/socialProfileStorage.ts') ? read('social/socialProfileStorage.ts') : ''
const social = read('social/SocialPrototype.tsx')
const provider = read('social/SocialProvider.tsx')
const rustSocial = fs.readFileSync(path.join(root, '..', 'src-tauri', 'src', 'social.rs'), 'utf8')
const css = read('social/SocialPrototype.css')
const en = read('i18n/en-US.ts')
const vi = read('i18n/vi-VN.ts')
const capability = fs.readFileSync(path.join(root, '..', 'src-tauri', 'capabilities', 'default.json'), 'utf8')

assert(storage.includes('BaseDirectory.AppLocalData'), 'profile must persist in AppLocalData')
assert(storage.includes('safeIdentitySegment') && storage.includes('pathsFor'), 'local profile storage must be isolated per Discord identity')
assert(storage.includes('profile.previous.json'), 'profile must keep a rolling metadata backup')
assert(storage.includes('cover.previous.bin'), 'profile must keep a rolling cover backup')
assert(storage.includes("from '@tauri-apps/plugin-dialog'"), 'cover picker must use native Tauri dialog')
assert(storage.includes('readFile') && storage.includes('writeFile'), 'cover persistence must use fs bytes')
assert(storage.includes('restorePreviousLocalSocialProfile'), 'profile backup must be restorable')
assert(social.includes('ProfileEditorModal'), 'profile customization editor missing')
assert(social.includes('serviceUnavailableLocal'), 'raw social transport failures must not leak into the profile UI')
assert(provider.includes('upsertFallbackSelf'), 'Discord fallback identity must replace the signed-out placeholder')
assert(provider.includes('Profile editing is local-first'), 'profile save must survive social backend outages')
assert(rustSocial.includes('Publishing is best-effort'), 'cover preview must not wait for remote publishing')
assert(social.includes('coverUrl'), 'profile surfaces must consume local cover URL')
assert(social.includes('t.social.changeCover') && en.includes('changeCover:') && vi.includes('changeCover:'), 'localized cover replacement control missing')
assert(social.includes('t.social.reposition') && en.includes('reposition:') && vi.includes('reposition:'), 'localized cover reposition control missing')
assert(css.includes('.social-cover-editor-preview'), 'cover editor preview styling missing')
assert(css.includes('.social-profile-banner.has-cover'), 'custom cover styling missing')
for (const permission of ['fs:allow-applocaldata-write-recursive', 'fs:allow-read-file', 'fs:allow-write-file', 'fs:allow-mkdir', 'fs:allow-remove']) {
  assert(capability.includes(permission), `missing ${permission} capability`)
}
console.log('profileCustomization.contract: PASS')
