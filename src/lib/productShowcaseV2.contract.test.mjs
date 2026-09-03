import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (rel) => fs.readFileSync(path.join(root, rel), 'utf8')
const assert = (condition, message) => { if (!condition) throw new Error(message) }

const view = read('components/WhatsNewView.tsx')
const css = read('components/WhatsNewView.css')

assert(view.includes('PRODUCT_CATALOG'), 'product showcase must be built around a product catalog, not only launcher features')
assert(view.includes('useTypewriter'), 'hero must include a typed/deleted rotating headline')
assert(view.includes('handlePointerMove'), 'showcase must react to pointer movement')
assert(view.includes('ContourField'), 'hero must include an animated contour field')
assert(view.includes('CinematicPicture'), 'feature showcase must use responsive screenshot media')
assert(view.includes('activeIndex'), 'cinematic showcase must keep a bounded media window')
assert(css.includes('.wn-contour-field'), 'contour background styling missing')
assert(css.includes('.wn-products-gallery'), 'product family gallery styling missing')
assert(css.includes('.wn-cinematic-showcase'), 'cinematic feature showcase missing')
assert(!view.includes('A launcher that feels'), 'old utility/product hero copy should be replaced')
console.log('productShowcaseV2.contract: PASS')
