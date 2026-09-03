export type TranslationSourceKey = 'theredteam' | 'canhcutteam' | 'gamethuanviet' | 'others';

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

export const VIETNAMESE_TRANSLATIONS_DATA: VietnameseTranslationItem[] = []
