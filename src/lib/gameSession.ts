import { invoke } from '@tauri-apps/api/core'

import type {
  AchievementState,
  GameSessionStateV1,
  OverlayMetricsSummary,
  OverlayRenderer,
} from '../types'

export function getGameSessionState(gameId: string): Promise<GameSessionStateV1> {
  return invoke<GameSessionStateV1>('get_game_session_state', { gameId })
}

export function listGameSessionStates(): Promise<GameSessionStateV1[]> {
  return invoke<GameSessionStateV1[]>('list_game_session_states')
}

export function getSessionAchievementState(gameId: string): Promise<AchievementState> {
  return invoke<AchievementState>('get_achievement_state', {
    gameId,
    installPath: null,
  })
}

export function getOverlayMetrics(gameId: string): Promise<OverlayMetricsSummary> {
  return invoke<OverlayMetricsSummary>('get_overlay_metrics', { gameId })
}

export function setOverlayProfile(
  gameId: string,
  renderer: OverlayRenderer,
): Promise<GameSessionStateV1> {
  return invoke<GameSessionStateV1>('set_overlay_profile', { gameId, renderer })
}
