import type { GameToolsCatalogItem, GameToolsProvider } from '../../types'

export type BypassProviderId = GameToolsProvider

export type BypassProviderMetadata = {
  id: BypassProviderId
  label: string
  catalogCategories: readonly string[]
  monogram: string
  accent: string
  description: string
}

export const BYPASS_PROVIDERS: readonly BypassProviderMetadata[] = [
  { id: 'ubisoft', label: 'Ubisoft', catalogCategories: ['UBISOFT'], monogram: 'U', accent: '#6ed7ff', description: 'Ubisoft Connect compatibility' },
  { id: 'ea', label: 'EA', catalogCategories: ['EA'], monogram: 'EA', accent: '#ff6b73', description: 'EA App compatibility' },
  { id: 'rockstar', label: 'Rockstar', catalogCategories: ['ROCKSTAR'], monogram: 'R★', accent: '#ffd23f', description: 'Rockstar Games Launcher compatibility' },
  { id: 'denuvo', label: 'Denuvo', catalogCategories: ['DENUVO'], monogram: 'D', accent: '#b897ff', description: 'Denuvo compatibility packages' },
  { id: 'playstation', label: 'PlayStation', catalogCategories: ['PLAYSTATION', 'PlayStation'], monogram: 'PS', accent: '#3f8cff', description: 'PlayStation PC compatibility' },
  { id: 'other', label: 'Other', catalogCategories: ['OTHERS', 'OTHER'], monogram: '…', accent: '#8be0b2', description: 'Other supported providers' },
] as const

export function providerForCategory(category: string | null): BypassProviderMetadata {
  const normalized = category?.trim().toLocaleUpperCase() ?? ''
  return BYPASS_PROVIDERS.find((provider) => provider.catalogCategories.some((value) => value.toLocaleUpperCase() === normalized))
    ?? BYPASS_PROVIDERS[BYPASS_PROVIDERS.length - 1]
}

export function itemsForProvider(items: readonly GameToolsCatalogItem[], providerId: BypassProviderId): GameToolsCatalogItem[] {
  return items.filter((item) => providerForCategory(item.category).id === providerId)
}

export function providerHero(items: readonly GameToolsCatalogItem[], providerId: BypassProviderId): string | null {
  const matching = itemsForProvider(items, providerId)
  return matching.find((item) => item.backgroundUrl)?.backgroundUrl
    ?? matching.find((item) => item.imageUrl)?.imageUrl
    ?? null
}
