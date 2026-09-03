import { invoke } from '@tauri-apps/api/core'

import type {
  LuaDriftResolution,
  LuaDriftResolutionResult,
  LuaFileDriftReport,
  LuaSourceProvider,
} from '../types'

export function checkLuaFileDrift(appId: number): Promise<LuaFileDriftReport> {
  return invoke<LuaFileDriftReport>('check_lua_file_drift', { appId })
}

export function resolveLuaDrift(request: {
  appId: number
  resolution: LuaDriftResolution
  provider?: LuaSourceProvider | null
  requestId?: string | null
  timezone?: string | null
  statSteamId?: string | null
}): Promise<LuaDriftResolutionResult> {
  return invoke<LuaDriftResolutionResult>('resolve_lua_drift', { request })
}
