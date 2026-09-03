import { invoke } from '@tauri-apps/api/core'
import type {
  GseUcComponentHealth,
  GseUcPlan,
  GseUcReceiptState,
} from '../types'

export function getGseUcResourceHealth() {
  return invoke<GseUcComponentHealth[]>('get_gse_uc_resource_health')
}

export function planGseUcSetup(appId: number) {
  return invoke<GseUcPlan>('plan_gse_uc_setup', { appId })
}

export function applyGseUcSetup(appId: number, expectedFingerprint: string) {
  return invoke<GseUcReceiptState>('apply_gse_uc_setup', { appId, expectedFingerprint })
}

export function verifyGseUcSetup(appId: number) {
  return invoke<GseUcReceiptState>('verify_gse_uc_setup', { appId })
}

export function repairGseUcSetup(appId: number, expectedFingerprint: string) {
  return invoke<GseUcReceiptState>('repair_gse_uc_setup', { appId, expectedFingerprint })
}

export function restoreGseUcSetup(appId: number) {
  return invoke<GseUcReceiptState>('restore_gse_uc_setup', { appId })
}

export function launchMigrateGse() {
  return invoke<number>('launch_migrate_gse')
}
