import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { SocialLeaderboardMetric, SocialMember, SocialRelationship } from './socialModels'

export type SocialProfileDto = {
  id: string
  username: string
  displayName: string
  avatarUrl: string
  bio: string
  customStatus: string
  accent: string
  coverUrl: string | null
  coverHash: string | null
  coverRevision: number
  coverPosition: number
  presence: 'playing' | 'online' | 'idle' | 'offline'
  activity: {
    kind: 'game' | 'launcher' | 'none'
    label: string
    detail: string
    gameId: string | null
    sinceMs: number
  }
  relationship: SocialRelationship | 'self'
  appearOffline?: boolean
  leaderboardOptIn: boolean
  stats: {
    downloads: number
    downloadedBytes: number
    playMinutes: number
    onlineMinutes: number
    gamesPlayed: number
  }
  joinedAt: string
}

export type SocialBootstrapDto = {
  serverTime: string
  presenceHeartbeatMs: number
  presenceStaleMs: number
  selfProfile: SocialProfileDto
  members: SocialProfileDto[]
  canary: boolean
  coverBatchMs: number
  offlineSnapshot: boolean
}

export type SocialCoverState = {
  localPath: string | null
  localHash: string | null
  publishedHash: string | null
  pending: boolean
  queuedAt: string | null
  publishBy: string | null
  lastError: string | null
  publicUploadConfirmed: boolean
  publishState: CoverPublishState
  removed: boolean
}

export type CoverPublishState = 'savedLocally' | 'pendingPublish' | 'published' | 'publishFailed'

export type SocialCoverDraftState = {
  draftId: string
  sha256: string
  mime: 'image/webp'
  width: number
  height: number
  byteLength: number
  publicUploadConfirmed: boolean
}

export type SocialProfileCoverOperation =
  | { operation: 'keep' }
  | { operation: 'replace'; sha256: string; mime: 'image/webp'; width: number; height: number }
  | { operation: 'remove' }

export type SocialProfileDraft = {
  draftId: string
  bio: string
  status: string
  accent: string
  coverPosition: number
  cover: SocialProfileCoverOperation
}

export type SocialServerEvent = {
  id: string
  type: 'presence.updated' | 'profile.updated' | 'relationship.updated' | 'cover.published' | 'cover.removed' | string
  createdAt: string
  payload: Record<string, unknown>
}

export type SocialLeaderboardEntry = {
  rank: number
  profile: SocialProfileDto
  score: number
}

export type SocialLeaderboardResult = {
  metric: 'downloads' | 'playMinutes' | 'onlineMinutes'
  period: 'all' | 'month' | 'week'
  scope: 'global' | 'friends'
  entries: SocialLeaderboardEntry[]
  communityDataNotice: boolean
}

let currentOnlineCount = 0
const countListeners = new Set<(count: number) => void>()

export function publishSocialOnlineCount(count: number) {
  const next = Math.max(0, Math.round(count))
  if (next === currentOnlineCount) return
  currentOnlineCount = next
  countListeners.forEach((listener) => listener(next))
}

export function subscribeSocialOnlineCount(listener: (count: number) => void) {
  countListeners.add(listener)
  listener(currentOnlineCount)
  return () => {
    countListeners.delete(listener)
  }
}

export function profileDtoToMember(profile: SocialProfileDto): SocialMember {
  const activityMinutes = profile.activity.sinceMs > 0
    ? Math.max(0, Math.round((Date.now() - profile.activity.sinceMs) / 60_000))
    : undefined
  return {
    id: profile.id,
    username: profile.username,
    displayName: profile.displayName,
    avatarUrl: profile.avatarUrl || undefined,
    accent: profile.accent || '42 88% 58%',
    presence: profile.presence === 'playing' ? 'online' : profile.presence,
    relationship: profile.relationship === 'self' ? 'friend' : profile.relationship,
    activity: {
      kind: profile.activity.kind,
      label: profile.activity.label,
      detail: profile.activity.detail || profile.customStatus,
      minutes: activityMinutes,
    },
    bio: profile.bio,
    joinedAt: profile.joinedAt,
    badges: profile.leaderboardOptIn ? ['Community leaderboard'] : [],
    stats: {
      downloads: profile.stats.downloads,
      downloadedGb: Math.round(profile.stats.downloadedBytes / 1024 ** 3),
      playMinutes: profile.stats.playMinutes,
      onlineMinutes: profile.stats.onlineMinutes,
      gamesPlayed: profile.stats.gamesPlayed,
    },
    recentGames: profile.activity.kind === 'game' && profile.activity.label ? [profile.activity.label] : [],
    coverUrl: profile.coverUrl || undefined,
    coverPosition: profile.coverPosition,
    coverHash: profile.coverHash || undefined,
    leaderboardOptIn: profile.leaderboardOptIn,
    appearOffline: profile.appearOffline === true,
  }
}

export async function getSocialBootstrap() {
  return invoke<SocialBootstrapDto>('get_social_bootstrap')
}

export async function searchSocialUsers(query: string, cursor?: string | null) {
  return invoke<{ items: SocialProfileDto[]; nextCursor: string | null }>('search_social_users', { query, cursor: cursor ?? null })
}

export async function updateSocialProfile(input: {
  bio: string
  customStatus: string
  accent: string
  coverPosition: number
  appearOffline: boolean
}) {
  return invoke<SocialProfileDto>('update_social_profile', { input })
}

export async function updateSocialRelationship(userId: string, current: SocialRelationship, next: SocialRelationship) {
  const requestId = crypto.randomUUID()
  if (next === 'blocked') return invoke('block_social_user', { userId, requestId })
  if (current === 'blocked' && next === 'none') return invoke('unblock_social_user', { userId, requestId })
  if (next === 'pending-out') return invoke('send_social_friend_request', { userId, requestId })
  if (current === 'pending-in' && next === 'friend') return invoke('accept_social_friend_request', { userId, requestId })
  if (current === 'pending-in' && next === 'none') return invoke('decline_social_friend_request', { userId, requestId })
  if (current === 'pending-out' && next === 'none') return invoke('cancel_social_friend_request', { userId, requestId })
  if (current === 'friend' && next === 'none') return invoke('remove_social_friend', { userId, requestId })
  throw new Error('Unsupported relationship transition.')
}

export function updateSocialPresence(input: {
  sessionId: string
  state: 'playing' | 'online' | 'idle' | 'offline'
  activityLabel: string
  activityDetail: string
  gameId?: string | null
  sinceMs: number
}) {
  return invoke('update_social_presence', { input })
}

export function getSocialLeaderboard(metric: SocialLeaderboardMetric, period: 'all' | 'month' | 'week', scope: 'global' | 'friends') {
  const serverMetric = metric === 'playtime' ? 'playMinutes' : metric === 'online' ? 'onlineMinutes' : 'downloads'
  return invoke<SocialLeaderboardResult>('get_social_leaderboard', { metric: serverMetric, period, scope })
}

export function setSocialLeaderboardParticipation(enabled: boolean) {
  return invoke<{ leaderboardOptIn: boolean; profileRevision: number }>('set_social_leaderboard_participation', { enabled })
}

export function recordSocialStats(input: {
  eventId: string
  kind: 'heartbeat' | 'game-session' | 'install'
  downloads?: number
  downloadedBytes?: number
  playMinutes?: number
  onlineMinutes?: number
  gamesPlayed?: number
}) {
  return invoke('record_social_stats', { input })
}

export function stageSocialCover(publicUploadConfirmed: boolean) {
  return invoke<SocialCoverDraftState | null>('stage_social_cover', { publicUploadConfirmed })
}

/** @deprecated Cover selection must be staged and committed through profile Save. */
export async function chooseSocialCover(publicUploadConfirmed: boolean): Promise<SocialCoverState | null> {
  void publicUploadConfirmed
  throw new Error('COVER_DRAFT_REQUIRED: Use stageSocialCover and commitSocialProfileDraft.')
}

export function getSocialCoverState() {
  return invoke<SocialCoverState>('get_social_cover_state')
}

export async function readSocialCoverPreview() {
  return new Uint8Array(await invoke<number[]>('read_social_cover_preview'))
}

export async function readSocialCoverDraftPreview(draftId: string) {
  return new Uint8Array(await invoke<number[]>('read_social_cover_draft_preview', { draftId }))
}

export function commitSocialProfileDraft(draft: SocialProfileDraft) {
  return invoke<SocialCoverState>('commit_social_profile_draft', { draft })
}

export function discardSocialProfileDraft(draftId: string) {
  return invoke<void>('discard_social_profile_draft', { draftId })
}

export function retrySocialCoverPublish() {
  return invoke<SocialCoverState>('retry_social_cover_publish')
}

export function migrateLegacySocialCover(bytes: Uint8Array, mime: string) {
  return invoke<SocialCoverState>('migrate_legacy_social_cover', { bytes: Array.from(bytes), mime })
}

/** @deprecated Cover removal is a draft operation committed by profile Save. */
export async function removeSocialCover(): Promise<never> {
  throw new Error('COVER_DRAFT_REQUIRED: Save a remove operation through commitSocialProfileDraft.')
}

export async function listenForSocialEvents(listener: (event: SocialServerEvent) => void): Promise<UnlistenFn> {
  return listen<SocialServerEvent>('social://event', ({ payload }) => listener(payload))
}

export async function listenForSocialCoverState(listener: (state: SocialCoverState) => void): Promise<UnlistenFn> {
  return listen<SocialCoverState>('social://cover-state', ({ payload }) => listener(payload))
}
