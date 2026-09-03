import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const read = (rel) => fs.readFileSync(path.join(root, rel), 'utf8')
const assert = (condition, message) => { if (!condition) throw new Error(message) }

const view = read('components/WhatsNewView.tsx')
const css = read('components/WhatsNewView.css')

assert(!view.includes('className="single-view whats-new-product-page"'), 'What\'s New must never inherit the generic two-column .single-view grid')
assert(view.includes('className="whats-new-product-page"'), 'What\'s New must own a dedicated page root')
assert(/\.whats-new-product-page\s*\{[\s\S]*?display:\s*block;/.test(css), 'showcase root must establish single-flow block layout')
assert(/\.whats-new-product-page\s*\{[\s\S]*?width:\s*100%;/.test(css), 'showcase root must fill the workspace width')
assert(/\.whats-new-product-page\s*\{[\s\S]*?padding:\s*0;/.test(css), 'showcase root must not inherit generic view padding')
assert(css.includes('.wn-product-exhibit') && css.includes('.wn-feature-story-sticky'), 'product and cinematic stages must own their layouts')
assert(/\.wn-feature-story-sticky\s*\{[\s\S]*?height:\s*100svh;/.test(css), 'cinematic stage must fill the internal viewport')
assert(/\.wn-highlights-section\s*\{[\s\S]*?width:\s*100%;/.test(css), 'cinematic chapters must use the full workspace width')
console.log('productShowcaseLayout.contract: PASS')
