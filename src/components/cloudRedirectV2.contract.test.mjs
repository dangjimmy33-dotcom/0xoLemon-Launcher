import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const root = path.resolve(here, '..')
const component = fs.readFileSync(path.join(here, 'CloudRedirectSettings.tsx'), 'utf8')
const overview = fs.readFileSync(path.join(here, 'CloudSavesOverview.tsx'), 'utf8')
const en = fs.readFileSync(path.join(root, 'i18n', 'en-US.ts'), 'utf8')
const vi = fs.readFileSync(path.join(root, 'i18n', 'vi-VN.ts'), 'utf8')

for (const command of [
  'cloud_redirect_engine_get_status',
  'cloud_redirect_engine_run_required_patches',
  'cloud_redirect_engine_save_provider',
  'cloud_redirect_engine_test_provider',
  'cloud_redirect_engine_list_apps',
  'cloud_redirect_engine_list_files',
  'cloud_redirect_engine_sync_app',
  'cloud_redirect_engine_sync_all',
  'cloud_redirect_engine_delete_app',
  'cloud_redirect_engine_get_manifest_pins',
  'cloud_redirect_engine_save_manifest_pins',
  'cloud_redirect_engine_create_backup',
  'cloud_redirect_engine_list_backups',
  'cloud_redirect_engine_restore_backup',
  'cloud_redirect_engine_list_stats',
  'cloud_redirect_engine_migrate',
  'cloud_redirect_engine_gc_blobs',
  'cloud_redirect_engine_publish_manifest',
  'cloud_redirect_engine_prune_legacy',
  'cloud_redirect_engine_run_cloud760',
  'cloud_redirect_engine_diagnostics',
]) assert.match(component, new RegExp(command), `missing frontend command ${command}`)

for (const provider of ['gdrive', 'onedrive', 'r2', 's3', 'folder', 'local']) {
  assert.match(component, new RegExp(`['\"]${provider}['\"]`), `missing provider ${provider}`)
}
assert.match(overview, /<CloudRedirectSettings\s*\/>/, 'CloudRedirect 2.6.3 UI is not mounted in Cloud Saves')
assert.match(component, /cloudredirect:\/\/migration-progress/, 'migration progress listener is missing')
assert.match(component, /'diagnostics'/, 'dedicated diagnostics tab is missing')
assert.match(component, /c\.attribution/, 'upstream attribution is not shown in the integrated UI')
assert.match(component, /function isLocalOnly\(provider: string\) \{ return provider === 'local' \}/, 'folder provider must remain available to upstream CLI inventory and migration')

function keys(source) {
  const start = source.indexOf('  cloudRedirectV2: {')
  const end = source.indexOf('\n  },\n  settings:', start)
  assert.ok(start >= 0 && end > start, 'cloudRedirectV2 i18n section missing')
  return [...source.slice(start, end).matchAll(/^    ([A-Za-z0-9_]+):/gm)].map((match) => match[1]).sort()
}
assert.deepEqual(keys(en), keys(vi), 'English and Vietnamese CloudRedirect keys differ')
assert.ok(keys(en).length >= 120, 'CloudRedirect translation surface is incomplete')
console.log(`CloudRedirect 2.6.3 contract PASS (${keys(en).length} bilingual keys)`)
