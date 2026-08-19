import type { GameCatalog } from '../types'

/**
 * GlobalChatSync (Disabled to prevent unnecessary Firestore reads).
 * Active chat is handled on-demand inside GameChat.tsx when user opens a game chat.
 */
export function GlobalChatSync({ catalog: _catalog }: { catalog: GameCatalog | null }) {
  return null
}

