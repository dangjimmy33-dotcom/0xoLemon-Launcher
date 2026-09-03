import { invoke } from '@tauri-apps/api/core'

import type { RestoreAndRelaunchResult } from '../types'

export type RestoreAndRelaunchRequest = {
  gameId: string
  snapshotId: string
  installPath: string
  launchExecutable?: string | null
  launchOptionId?: string | null
}

export function restoreAndRelaunch(
  request: RestoreAndRelaunchRequest,
): Promise<RestoreAndRelaunchResult> {
  return invoke<RestoreAndRelaunchResult>('restore_and_relaunch', request)
}
