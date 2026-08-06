import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const downloads = await readFile(new URL('./downloads.tsx', import.meta.url), 'utf8')
const translations = await readFile(new URL('./TranslationsView.tsx', import.meta.url), 'utf8')
const activeView = await readFile(new URL('./ActiveView.tsx', import.meta.url), 'utf8')
const app = await readFile(new URL('../App.tsx', import.meta.url), 'utf8')
const dock = await readFile(new URL('./TransferDock.tsx', import.meta.url), 'utf8')

for (const label of ['Already available', 'This session', 'Remaining']) {
  assert.ok(downloads.includes(label), `downloads view must include ${label}`)
}
assert.ok(downloads.includes('aria-valuenow'), 'downloads progress must expose determinate accessibility values')
assert.ok(downloads.includes('details className="job-log-disclosure"'), 'job log must be progressively disclosed')

assert.ok(dock.includes('transfer-dock'), 'floating transfer dock component must exist')
assert.ok(dock.includes('Open transfer details'), 'dock must expose an accessible navigation action')
assert.ok(app.includes('<TransferDock'), 'App must mount the global transfer dock')

assert.ok(translations.includes('Translation catalog'), 'translations must have a catalog mode')
assert.ok(translations.includes('Search games or translations'), 'translations must support search')
assert.ok(translations.includes('translation-catalog-card'), 'translations must render catalog cards')
assert.ok(translations.includes('Available archives'), 'translation detail must list archives')
assert.ok(!translations.includes("disabled={state?.status === 'ready' && !available && !state.installed}"), 'unavailable games must still open their detail page')
assert.ok(activeView.includes('catalog={catalog}'), 'ActiveView must pass the whole catalog to TranslationsView')

console.log('UI contract tests passed')
