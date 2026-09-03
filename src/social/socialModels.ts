export type SocialPresence = 'online' | 'idle' | 'offline'
export type SocialRelationship = 'friend' | 'pending-in' | 'pending-out' | 'none' | 'blocked'
export type SocialLeaderboardMetric = 'downloads' | 'playtime' | 'online'
export type SocialLeaderboardPeriod = 'all' | 'month' | 'week' | 'friends'

export type SocialStats = {
  downloads: number
  downloadedGb: number
  playMinutes: number
  onlineMinutes: number
  gamesPlayed: number
}

export type SocialActivity = {
  kind: 'game' | 'launcher' | 'none'
  label: string
  detail?: string
  minutes?: number
}

export type SocialMember = {
  id: string
  username: string
  displayName: string
  avatarUrl?: string
  accent: string
  presence: SocialPresence
  relationship: SocialRelationship
  activity: SocialActivity
  bio: string
  joinedAt: string
  badges: string[]
  stats: SocialStats
  recentGames: string[]
  coverUrl?: string
  coverPosition?: number
  coverHash?: string
  leaderboardOptIn?: boolean
  appearOffline?: boolean
}

export function formatSocialMinutes(totalMinutes: number): string {
  const hours = Math.max(0, Math.round(totalMinutes / 60))
  if (hours < 1000) return `${hours}h`
  return `${(hours / 1000).toFixed(1)}k h`
}
