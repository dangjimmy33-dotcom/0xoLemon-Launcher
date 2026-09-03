import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const view = readFileSync(new URL('../components/TranslationsView.tsx', import.meta.url), 'utf8')
const css = readFileSync(new URL('../components/TranslationsView.css', import.meta.url), 'utf8')
const english = readFileSync(new URL('../i18n/en-US.ts', import.meta.url), 'utf8')
const vietnamese = readFileSync(new URL('../i18n/vi-VN.ts', import.meta.url), 'utf8')
const bootstrap = readFileSync(new URL('../desktop/bootstrap.tsx', import.meta.url), 'utf8')

assert.ok(view.includes('const { t } = useLocale()'), 'Translations must use the launcher locale')
assert.ok(view.includes('const copy = t.translationsView'), 'Translations copy must come from locale contracts')
assert.ok(english.includes('translationsView: {'), 'English translation copy is required')
assert.ok(vietnamese.includes('translationsView: {'), 'Vietnamese translation copy is required')
for (const hardcodedCopy of ['Nguồn Khác', 'Tất cả (', 'Đã cài game', 'Không tìm thấy bản dịch', 'Xem chi tiết']) {
  assert.ok(!view.includes(hardcodedCopy), `visible copy must not be hardcoded in the view: ${hardcodedCopy}`)
}

assert.ok(view.includes("type TranslationGridColumns = 4 | 6 | 8"), 'grid density must reject stale unsupported values')
assert.ok(css.includes('grid-template-columns: repeat(var(--translation-grid-columns, 6), minmax(0, 1fr))'), 'the catalog must own full-width grid tracks')
assert.ok(css.includes('.translation-lightning-grid > .translation-catalog-card.lightning-game-card'), 'legacy card styles must not collapse Lightning cards')
assert.ok(view.includes('translationIdentity(item)'), 'duplicate upstream item IDs must be namespaced by source and archive URL')

assert.ok(view.includes('translation-backdrop-image'), 'provider backgrounds must crossfade on independent layers')
assert.ok(css.includes('opacity 460ms cubic-bezier'), 'provider background crossfade must be animated')
assert.ok(view.includes('onFocus={() => setHoveredSource(source.key)}'), 'provider previews must work from keyboard focus')
assert.ok(css.includes('@media (prefers-reduced-motion: reduce)'), 'translation motion must expose a reduced-motion profile')
assert.ok(bootstrap.includes("fixture === 'translations'"), 'Translations must expose a development-only visual QA fixture')

console.log('translationsView.contract: PASS')
