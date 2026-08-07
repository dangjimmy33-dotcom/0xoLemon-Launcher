import type { CloudSaveQuota, CloudSaveStatus } from '../types'

export type CloudSaveTone = 'success' | 'info' | 'warning' | 'danger' | 'neutral'

export type CloudSavePresentation = {
  title: string
  description: string
  tone: CloudSaveTone
  blocking: boolean
}

type StateCopy = { title: string; description: string }

const STATE_META: Record<string, Pick<CloudSavePresentation, 'tone' | 'blocking'>> = {
  synced: { tone: 'success', blocking: false },
  ready: { tone: 'success', blocking: false },
  syncing: { tone: 'info', blocking: false },
  offline: { tone: 'warning', blocking: false },
  rate_limited: { tone: 'warning', blocking: false },
  storage_full: { tone: 'warning', blocking: false },
  auth_required: { tone: 'warning', blocking: false },
  waiting_for_first_save: { tone: 'neutral', blocking: false },
  waiting_for_save: { tone: 'info', blocking: false },
  conflict_check_required: { tone: 'warning', blocking: false },
  permission_denied: { tone: 'warning', blocking: false },
  conflict: { tone: 'danger', blocking: true },
  remote_damaged: { tone: 'danger', blocking: true },
  disabled: { tone: 'neutral', blocking: false },
}

const FALLBACK_COPY: Record<string, StateCopy> = {
  synced: { title: 'Synced', description: 'The latest save is protected on this PC and Google Drive.' },
  ready: { title: 'Ready to protect', description: 'The launcher checks before play and syncs after the game exits.' },
  syncing: { title: 'Protecting save', description: 'You can keep using the launcher normally.' },
  offline: { title: 'Waiting for connection', description: 'The latest save is protected on this PC and will sync automatically.' },
  rate_limited: { title: 'Google Drive is busy', description: 'The task is queued and will retry automatically.' },
  storage_full: { title: 'Google Drive is full', description: 'The save is safe on this PC but cannot upload yet.' },
  auth_required: { title: 'Connect Google Drive', description: 'The save is protected locally. Connect once to sync across devices.' },
  waiting_for_first_save: { title: 'Waiting for the first save', description: 'The launcher will detect it when the game creates progress data.' },
  waiting_for_save: { title: 'Waiting for the game to finish saving', description: 'A safe local copy is kept until the files become stable.' },
  conflict_check_required: { title: 'Checking a newer save', description: 'Cloud changed on another device, so nothing is overwritten yet.' },
  permission_denied: { title: 'Reconnect Google Drive', description: 'The local save is safe, but Drive authorization is no longer valid.' },
  conflict: { title: 'Choose a progress version', description: 'Both the local and Cloud versions were preserved safely.' },
  remote_damaged: { title: 'Cloud Save needs attention', description: 'Local data was not changed; restore stopped for safety.' },
  disabled: { title: 'Automatic protection is off', description: 'The save is kept only on this PC.' },
}

export function cloudSavePresentation(
  status: Pick<CloudSaveStatus, 'state' | 'lastMessage'> | null,
  localizedCopy?: Readonly<Record<string, Readonly<StateCopy>>>,
): CloudSavePresentation {
  const state = status?.state || 'disabled'
  const meta = STATE_META[state] ?? { tone: 'neutral' as const, blocking: false }
  const copy = localizedCopy?.[state] ?? FALLBACK_COPY[state]
  if (copy) return { ...copy, ...meta }
  return {
    title: 'Cloud Save',
    description: status?.lastMessage || 'The launcher is checking save status.',
    ...meta,
  }
}

export function quotaPercent(quota: Pick<CloudSaveQuota, 'limitBytes' | 'usageBytes'> | null): number | null {
  if (!quota || quota.limitBytes == null || !Number.isFinite(quota.limitBytes) || quota.limitBytes <= 0) return null
  return Math.max(0, Math.min(100, Math.round((quota.usageBytes / quota.limitBytes) * 100)))
}

export function pendingSummary(status: Pick<CloudSaveStatus, 'pendingOperationCount' | 'pendingUploadBytes'> | null) {
  if (!status?.pendingOperationCount) return null
  return { count: status.pendingOperationCount, bytes: status.pendingUploadBytes }
}
