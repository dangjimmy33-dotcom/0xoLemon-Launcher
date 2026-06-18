import gameTagTable from '../../src-tauri/game-tags.json'

export type GameTagTone = 'danger' | 'online' | 'offline' | string

export type GameTag = {
  id: string
  label: string
  tone: GameTagTone
}

type GameTagTable = {
  schemaVersion: number
  definitions: Record<string, { label: string; tone: GameTagTone }>
  games: Record<string, string[]>
}

const table = gameTagTable as GameTagTable

export function getGameTags(gameId: string): GameTag[] {
  const configured = table.games[gameId] ?? []
  return configured.flatMap((id) => {
    const definition = table.definitions[id]
    return definition ? [{ id, label: definition.label, tone: definition.tone }] : []
  })
}

export function gameHasTag(gameId: string, tagId: string) {
  return (table.games[gameId] ?? []).some((value) => value.toLowerCase() === tagId.toLowerCase())
}
