import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (rel) => fs.readFileSync(path.join(root, rel), 'utf8')
const assert = (condition, message) => { if (!condition) throw new Error(message) }

const view = read('components/WhatsNewView.tsx')
const css = read('components/WhatsNewView.css')

assert(view.includes('target: ref'), 'Reveal must use target-based scroll progress')
assert(view.includes("offset: ['start 94%', 'end 16%']"), 'Reveal must use bounded internal scroll offsets')
assert(view.includes('revealOpacity'), 'scroll-linked reveal opacity missing')
assert(view.includes('revealY'), 'scroll-linked reveal translation missing')
assert(view.includes('heroY'), 'hero scroll translation missing')
assert(view.includes('heroScale'), 'hero scroll scale missing')
assert(view.includes('productParallaxY'), 'product mockup parallax missing')
assert(view.includes('productParallaxScale'), 'product mockup scale motion missing')
assert(view.includes('mediaMask') && view.includes('clipPath: mediaMask'), 'chapter crop/mask motion missing')
assert(view.includes('copyOpacity') && view.includes('copyY'), 'chapter copy reveal motion missing')
assert(view.includes('shouldMountMedia') && view.includes('Math.abs(index - activeIndex) <= 1'), 'offscreen chapter media must be unloaded')
assert(css.includes('will-change: transform, opacity'), 'showcase motion surfaces need compositor hint')
console.log('productShowcaseScrollMotion.contract: PASS')
