import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (rel) => fs.readFileSync(path.join(root, rel), 'utf8')
const exists = (rel) => fs.existsSync(path.join(root, rel))
const assert = (condition, message) => { if (!condition) throw new Error(message) }

assert(exists('components/WhatsNewView.css'), 'missing product showcase stylesheet')
const view = read('components/WhatsNewView.tsx')
const css = read('components/WhatsNewView.css')
assert(view.includes('PRODUCT_FEATURES'), 'What\'s New must remain data-driven for future feature stories')
assert(view.includes('PRODUCT_CATALOG'), 'What\'s New must support multiple products')
assert(view.includes('useScroll') && view.includes('useTransform') && view.includes('useSpring'), 'showcase must keep Motion primitives for progress/pointer motion')
assert(view.includes('ProductShowcaseVisual'), 'missing reusable animated product visual component')
assert(view.includes('CinematicShowcase'), 'missing cinematic feature showcase')
assert(view.includes('FeatureStoryChapter'), 'missing scroll-driven feature chapters')
assert(view.includes('CinematicPicture'), 'missing responsive real launcher media')
assert(view.includes('release-history'), 'release history anchor missing')
assert(view.includes('isModal'), 'compact modal behavior must remain supported')
assert(css.includes('.wn-hero'), 'product hero styling missing')
assert(css.includes('.wn-cinematic-showcase'), 'cinematic showcase styling missing')
assert(css.includes('.wn-feature-story-sticky'), 'sticky chapter stage styling missing')
assert(css.includes('.whats-new-release-list'), 'release history styling missing')
assert(css.includes('@media (prefers-reduced-motion: reduce)'), 'showcase must respect reduced motion')
console.log('productShowcase.contract: PASS')
