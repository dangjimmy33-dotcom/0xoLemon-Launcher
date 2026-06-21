import gameTagTableDefault from '../../src-tauri/game-tags.json'
import type { GameSummary } from '../types'

export type GameTagTone = 'danger' | 'online' | 'offline' | 'warning' | string

export type GameTag = {
  id: string
  label: string
  tone: GameTagTone
}

export type GameTagTable = {
  schemaVersion: number
  definitions: Record<string, { label: string; tone: GameTagTone }>
  games: Record<string, string[]>
}

type GameTagTarget = string | Pick<GameSummary, 'id' | 'title' | 'assetPackPath'>

let table = { ...gameTagTableDefault } as GameTagTable
let gameTagIndex = new Map<string, string[]>()

function rebuildIndex() {
  gameTagIndex.clear()
  for (const [gameId, tags] of Object.entries(table.games)) {
    gameTagIndex.set(gameId, tags)
    gameTagIndex.set(normalizeLookupKey(gameId), tags)
  }
}

// Initial build
rebuildIndex()

export function updateGameTagTable(newTable: GameTagTable) {
  table = newTable
  rebuildIndex()
}

function normalizeLookupKey(value: string) {
  return value
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
}

function packGameId(assetPackPath: string | undefined) {
  if (!assetPackPath) return ''
  const normalized = assetPackPath.replace(/\\/g, '/')
  const match = normalized.match(/(?:^|\/)assets\/games\/([^/]+)(?:\/|$)/i)
  return match?.[1] ?? ''
}

function candidateGameIds(target: GameTagTarget) {
  if (typeof target === 'string') {
    return [target, normalizeLookupKey(target)].filter(Boolean)
  }

  const values = [
    target.id,
    normalizeLookupKey(target.id),
    target.title,
    normalizeLookupKey(target.title),
    packGameId(target.assetPackPath),
    normalizeLookupKey(packGameId(target.assetPackPath)),
  ]

  return [...new Set(values.filter(Boolean))]
}

function configuredTagIds(target: GameTagTarget) {
  for (const candidate of candidateGameIds(target)) {
    const configured = gameTagIndex.get(candidate) ?? gameTagIndex.get(normalizeLookupKey(candidate))
    if (configured) return configured
  }
  return []
}

export function getGameTags(target: GameTagTarget): GameTag[] {
  return configuredTagIds(target).flatMap((id) => {
    const definition = table.definitions[id]
    return definition ? [{ id, label: definition.label, tone: definition.tone }] : []
  })
}

export function gameHasTag(target: GameTagTarget, tagId: string) {
  const wanted = tagId.trim().toLowerCase()
  return configuredTagIds(target).some((value) => value.trim().toLowerCase() === wanted)
}
