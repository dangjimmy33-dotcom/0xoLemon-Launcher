import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (rel) => fs.readFileSync(path.join(root, rel), 'utf8')
const assert = (condition, message) => { if (!condition) throw new Error(message) }

const view = read('components/WhatsNewView.tsx')
const css = read('components/WhatsNewView.css')

assert(view.includes('function CinematicShowcase'), 'feature stories must live in one cinematic showcase component')
assert(view.includes('function FeatureStoryChapter'), 'feature stories must use isolated scroll chapters')
assert(view.includes('useInView'), 'sections/showcase must observe the internal launcher scroll container')
assert(view.includes('root: pageRef') || view.includes('root: pageRef,'), 'chapter visibility must use the internal What\'s New scroll root')
assert(!view.includes('setInterval'), 'cinematic storytelling must never autoplay')
assert(view.includes("offset: ['start end', 'end start']"), 'chapters must map the full reversible scroll range')
assert(css.includes('height: 210svh'), 'desktop chapters need a deliberate multi-viewport scroll range')
assert(css.includes('.wn-feature-story-sticky') && css.includes('position: sticky'), 'each chapter needs a sticky full-screen stage')
assert(!css.includes('scroll-snap'), 'cinematic chapters must not force scroll snapping')
assert(css.includes('.whats-new-release-section'), 'release notes section missing')
console.log('productShowcaseFlow.contract: PASS')
