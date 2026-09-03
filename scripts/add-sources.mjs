import fs from 'node:fs'

const content = fs.readFileSync('src/data/vietnameseTranslations.ts', 'utf8')

// Parse the array in VIETNAMESE_TRANSLATIONS_DATA
const match = content.match(/export const VIETNAMESE_TRANSLATIONS_DATA: VietnameseTranslationItem\[\] = (\[[\s\S]*?\]);\s*$/)
if (!match) {
  console.error('Could not find data array')
  process.exit(1)
}

const list = JSON.parse(match[1])

list.forEach(item => {
  const fn = (item.fileName || '').toLowerCase()
  const path = (item.downloadUrl || '').toLowerCase()
  
  if (fn.includes('canhcut') || fn.includes('cct') || path.includes('canhcut')) {
    item.source = 'canhcutteam'
    if (!item.author) item.author = 'Cánh Cụt Team'
  } else if (fn.includes('redteam') || fn.includes('trt') || path.includes('redteam')) {
    item.source = 'theredteam'
    if (!item.author) item.author = 'The Red Team'
  } else if (fn.includes('thuanviet') || fn.includes('gtv') || fn.includes('gametiengviet')) {
    item.source = 'gamethuanviet'
    if (!item.author) item.author = 'Game Thuần Việt'
  } else {
    item.source = 'others'
  }
})

const fileContent = `export type TranslationSourceKey = 'theredteam' | 'canhcutteam' | 'gamethuanviet' | 'others';

export interface VietnameseTranslationItem {
  id: string
  gameTitle: string
  translationTitle: string
  fileName?: string
  author: string
  version: string
  size: string
  downloadUrl: string
  coverUrl?: string
  bannerUrl?: string
  description: string
  installGuide?: string
  tags: string[]
  repo?: string
  gameId?: string
  source?: TranslationSourceKey
  downloads?: number
  likes?: number
  updatedAt?: string
  isRecommended?: boolean
}

export const VIETNAMESE_TRANSLATIONS_DATA: VietnameseTranslationItem[] = ${JSON.stringify(list, null, 2)};
`

fs.writeFileSync('src/data/vietnameseTranslations.ts', fileContent, 'utf8')
console.log('Updated vietnameseTranslations.ts with sources successfully!')
