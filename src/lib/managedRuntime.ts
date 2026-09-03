import { invoke } from '@tauri-apps/api/core'
import type { ManagedGseState, ManagedRuntimePlanV2 } from '../types'

export type ManagedRuntimeTargetInput = {
  gameId: string
  installDir: string
  launchExecutable: string
}

export function planManagedRuntime(input: ManagedRuntimeTargetInput) {
  return invoke<ManagedRuntimePlanV2>('plan_managed_runtime', input)
}

export function applyManagedRuntime(input: ManagedRuntimeTargetInput) {
  return invoke<ManagedGseState>('apply_managed_runtime', input)
}

export function verifyManagedRuntime(input: ManagedRuntimeTargetInput) {
  return invoke<ManagedGseState>('verify_managed_runtime', input)
}

export function repairManagedRuntime(input: ManagedRuntimeTargetInput) {
  return invoke<ManagedGseState>('repair_managed_runtime', input)
}

export function restoreManagedRuntime(input: ManagedRuntimeTargetInput) {
  return invoke<ManagedGseState>('restore_managed_runtime', input)
}
