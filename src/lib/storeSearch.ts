import type { GameInstallState, GameSummary } from '../types'
import { getGameTags } from './gameTags'

export type StoreSearchFilter = 'all' | 'installed' | 'popular' | 'new' | 'downloaded'

export type StoreSearchResult = {
  game: GameSummary
  score: number
  matchLabel: string
}

type StoreSearchOptions = {
  games: GameSummary[]
  query: string
  filter: StoreSearchFilter
  installStates?: Record<string, GameInstallState>
  steamInstalledAppIds?: number[]
  steamMapping: Record<string, number>
  downloads: Record<string, number>
  likes: Record<string, number>
  searchClicks: Record<string, number>
}

const WORD_SEPARATOR = /[^a-z0-9]+/g

export function normalizeStoreSearchTerm(value: string): string {
  return value
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(WORD_SEPARATOR, ' ')
    .trim()
    .replace(/\s+/g, ' ')
}

function editDistance(left: string, right: string): number {
  if (left === right) return 0
  if (!left.length) return right.length
  if (!right.length) return left.length

  const previous = Array.from({ length: right.length + 1 }, (_, index) => index)
  const current = new Array<number>(right.length + 1)

  for (let leftIndex = 1; leftIndex <= left.length; leftIndex += 1) {
    current[0] = leftIndex
    for (let rightIndex = 1; rightIndex <= right.length; rightIndex += 1) {
      const substitution = previous[rightIndex - 1] + (left[leftIndex - 1] === right[rightIndex - 1] ? 0 : 1)
      current[rightIndex] = Math.min(
        current[rightIndex - 1] + 1,
        previous[rightIndex] + 1,
        substitution,
      )
    }
    for (let index = 0; index <= right.length; index += 1) previous[index] = current[index]
  }

  return previous[right.length]
}

function tokenMatches(queryToken: string, candidateToken: string): boolean {
  if (candidateToken.startsWith(queryToken) || candidateToken.includes(queryToken)) return true
  if (queryToken.length < 4 || candidateToken.length < 4) return false
  const allowedDistance = Math.max(queryToken.length, candidateToken.length) >= 8 ? 2 : 1
  return editDistance(queryToken, candidateToken) <= allowedDistance
}

function matchScore(game: GameSummary, normalizedQuery: string, mappedAppId?: number): { score: number; label: string } | null {
  if (!normalizedQuery) return { score: 0, label: 'Recommended' }

  const title = normalizeStoreSearchTerm(game.title)
  const appId = String(game.appid ?? '')
  const steamAppId = mappedAppId ? String(mappedAppId) : ''
  const mappedTags = getGameTags(game).map((tag) => normalizeStoreSearchTerm(tag.label || tag.id))
  const secondaryFields = [
    game.id,
    appId,
    steamAppId,
    game.subtitle,
    game.developer,
    game.publisher,
    ...game.availableVersions.flatMap((version) => [version.version, version.label, version.buildId]),
    ...mappedTags,
  ].map(normalizeStoreSearchTerm)
  const queryTokens = normalizedQuery.split(' ')
  const titleTokens = title.split(' ')
  const allTokens = [title, ...secondaryFields].join(' ').split(' ').filter(Boolean)

  if (appId && appId === normalizedQuery) return { score: 1_400, label: 'Exact AppID' }
  if (steamAppId && steamAppId === normalizedQuery) return { score: 1_400, label: 'Exact Steam AppID' }
  if (normalizeStoreSearchTerm(game.id) === normalizedQuery) return { score: 1_300, label: 'Exact game ID' }
  if (title === normalizedQuery) return { score: 1_250, label: 'Exact title' }
  if (title.startsWith(normalizedQuery)) return { score: 1_050, label: 'Title match' }
  if (title.includes(normalizedQuery)) return { score: 900, label: 'Title match' }

  const titleMatches = queryTokens.filter((queryToken) => titleTokens.some((candidate) => tokenMatches(queryToken, candidate))).length
  if (titleMatches === queryTokens.length) return { score: 760 + titleMatches * 24, label: 'Title match' }

  const allMatches = queryTokens.filter((queryToken) => allTokens.some((candidate) => tokenMatches(queryToken, candidate))).length
  if (allMatches === queryTokens.length) {
    const tagMatch = mappedTags.some((tag) => tag.includes(normalizedQuery))
    return { score: 560 + allMatches * 18, label: tagMatch ? 'Tag match' : 'Related match' }
  }

  return null
}

function isInstalled(
  game: GameSummary,
  installStates: Record<string, GameInstallState> | undefined,
  steamInstalledAppIds: number[] | undefined,
  steamMapping: Record<string, number>,
): boolean {
  if (installStates?.[game.id]?.installed) return true
  const appId = steamMapping[game.id]
  return Boolean(appId && steamInstalledAppIds?.includes(appId))
}

export function rankStoreGames(options: StoreSearchOptions): StoreSearchResult[] {
  const normalizedQuery = normalizeStoreSearchTerm(options.query)
  const newestIds = new Set(options.games.slice(-Math.min(12, Math.max(1, Math.ceil(options.games.length / 4)))).map((game) => game.id))

  const ranked = options.games.flatMap((game) => {
    if (options.filter === 'installed' && !isInstalled(game, options.installStates, options.steamInstalledAppIds, options.steamMapping)) {
      return []
    }
    if (options.filter === 'new' && !newestIds.has(game.id)) return []

    const match = matchScore(game, normalizedQuery, options.steamMapping[game.id])
    if (!match) return []

    const downloads = options.downloads[game.id] || 0
    const likes = options.likes[game.id] || 0
    const clicks = options.searchClicks[game.id] || 0
    const popularity = Math.log10(downloads + 1) * 18 + Math.log10(likes + 1) * 12 + Math.log10(clicks + 1) * 14
    const installedBoost = isInstalled(game, options.installStates, options.steamInstalledAppIds, options.steamMapping) ? 8 : 0
    const newBoost = newestIds.has(game.id) ? 6 : 0

    let modeBoost = 0
    if (options.filter === 'popular') modeBoost = Math.log10(downloads + likes * 3 + clicks * 2 + 1) * 120
    if (options.filter === 'downloaded') modeBoost = Math.log10(downloads + 1) * 150

    return [{
      game,
      matchLabel: match.label,
      score: match.score + popularity + installedBoost + newBoost + modeBoost,
    }]
  })

  return ranked.sort((left, right) => right.score - left.score || left.game.title.localeCompare(right.game.title))
}
