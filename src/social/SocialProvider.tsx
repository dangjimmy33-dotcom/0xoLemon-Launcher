import {
  createContext,
  useCallback,
  useEffect,
  useContext,
  useMemo,
  useRef,
  useState,
  type PropsWithChildren,
} from 'react'
import type { DiscordAuthUser } from '../types'
import type { SocialMember, SocialRelationship } from './socialModels'
import {
  commitSocialProfileDraft,
  discardSocialProfileDraft,
  getSocialBootstrap,
  getSocialCoverState,
  listenForSocialEvents,
  listenForSocialCoverState,
  migrateLegacySocialCover,
  profileDtoToMember,
  publishSocialOnlineCount,
  readSocialCoverDraftPreview,
  readSocialCoverPreview,
  recordSocialStats,
  retrySocialCoverPublish,
  setSocialLeaderboardParticipation,
  stageSocialCover,
  updateSocialPresence,
  updateSocialProfile,
  updateSocialRelationship,
  type CoverPublishState,
  type SocialCoverDraftState,
  type SocialCoverState,
  type SocialProfileCoverOperation,
  type SocialServerEvent,
} from './socialApi'
import {
  createDefaultLocalSocialProfile,
  loadLocalSocialProfile,
  restorePreviousLocalSocialProfile,
  saveLocalSocialProfile,
  type LocalSocialProfileDocument,
  type LocalSocialProfileSnapshot,
} from './socialProfileStorage'

export type SocialPrototypeContextValue = {
  selfId: string
  members: SocialMember[]
  drawerOpen: boolean
  drawerWidth: number
  search: string
  profileUserId: string | null
  fullProfileUserId: string | null
  localProfile: LocalSocialProfileDocument
  coverUrl: string | null
  coverDraft: SocialCoverDraftState | null
  coverDraftUrl: string | null
  coverPublishState: CoverPublishState
  coverPublishError: string | null
  coverPublishBy: string | null
  profileEditorOpen: boolean
  loading: boolean
  error: string | null
  publicCoverConfirmed: boolean
  migrationPending: boolean
  setDrawerOpen: (open: boolean) => void
  setDrawerWidth: (width: number) => void
  setSearch: (query: string) => void
  openProfile: (userId: string) => void
  openFullProfile: (userId: string) => void
  closeProfile: () => void
  openProfileEditor: () => void
  closeProfileEditor: () => void
  saveSelfProfile: (profile: LocalSocialProfileDocument, cover: SocialProfileCoverOperation) => Promise<CoverPublishState>
  restoreSelfProfile: () => Promise<boolean>
  chooseSelfCover: (publicUploadConfirmed: boolean) => Promise<SocialCoverDraftState | null>
  discardSelfCoverDraft: () => Promise<void>
  retrySelfCoverPublish: () => Promise<CoverPublishState>
  registerMembers: (members: SocialMember[]) => void
  setLeaderboardParticipation: (enabled: boolean) => Promise<void>
  setAppearOffline: (enabled: boolean) => Promise<void>
  migrateLocalProfile: () => Promise<void>
  dismissLocalProfileMigration: () => void
  updateRelationship: (userId: string, relationship: SocialRelationship) => void
}

export const SocialPrototypeContext = createContext<SocialPrototypeContextValue | null>(null)

export function useSocialPrototype() {
  const value = useContext(SocialPrototypeContext)
  if (!value) throw new Error('Social components must be inside SocialPrototypeProvider')
  return value
}

const STORAGE_PREFIX = '0xo_social_proto_v1'
const WIDTH_MIN = 268
const WIDTH_MAX = 420
const WIDTH_DEFAULT = 312

function clampWidth(value: number) {
  return Math.min(WIDTH_MAX, Math.max(WIDTH_MIN, value))
}

function safeLocalStorageGet(key: string) {
  try {
    return window.localStorage.getItem(key)
  } catch {
    return null
  }
}

function safeLocalStorageSet(key: string, value: string) {
  try {
    window.localStorage.setItem(key, value)
  } catch {
    // Social preferences remain functional when persistent browser storage is unavailable.
  }
}

function upsertFallbackSelf(
  members: SocialMember[],
  fallbackSelf: SocialMember,
  selfId: string,
) {
  const fallbackId = selfId || fallbackSelf.id
  const existing = members.find((member) => member.id === fallbackId)
  const self = existing ? {
    ...fallbackSelf,
    ...existing,
    id: fallbackId,
    username: fallbackSelf.username,
    displayName: fallbackSelf.displayName,
    avatarUrl: fallbackSelf.avatarUrl || existing.avatarUrl,
  } : fallbackSelf

  return [
    self,
    ...members.filter((member) => member.id !== fallbackId && member.id !== 'signed-out'),
  ]
}

export function SocialPrototypeProvider({
  user,
  activeGame,
  children,
}: PropsWithChildren<{
  user: DiscordAuthUser | null
  activeGame: { gameId: string; label: string } | null
}>) {
  const selfId = user?.id ?? ''
  const fallbackSelf = useMemo<SocialMember>(() => ({
    id: selfId || 'signed-out',
    username: user?.username ?? 'you',
    displayName: user?.displayName ?? 'You',
    avatarUrl: user?.avatarUrl || undefined,
    accent: '42 88% 58%',
    presence: 'online',
    relationship: 'friend',
    activity: { kind: 'launcher', label: 'Using 0xoLemon' },
    bio: '',
    joinedAt: user?.accountCreatedAt ?? new Date(0).toISOString(),
    badges: [],
    stats: { downloads: 0, downloadedGb: 0, playMinutes: 0, onlineMinutes: 0, gamesPlayed: 0 },
    recentGames: [],
  }), [selfId, user])
  const [remoteMembers, setRemoteMembers] = useState<SocialMember[]>(() => [fallbackSelf])
  const [loading, setLoading] = useState(Boolean(user))
  const [error, setError] = useState<string | null>(null)
  const [serverSelfProfile, setServerSelfProfile] = useState<ReturnType<typeof profileDtoToMember> | null>(null)
  const [localProfileLoaded, setLocalProfileLoaded] = useState(false)
  const [publicCoverConfirmed, setPublicCoverConfirmed] = useState(false)
  const [migrationDecision, setMigrationDecision] = useState<string | null>(() => (
    typeof window !== 'undefined' ? safeLocalStorageGet(`${STORAGE_PREFIX}:profile-migration:${selfId}`) : null
  ))
  const defaultLocalProfile = useMemo(
    () => createDefaultLocalSocialProfile(selfId || 'signed-out', ''),
    [selfId],
  )
  const [localProfile, setLocalProfile] = useState<LocalSocialProfileDocument>(defaultLocalProfile)
  const [coverUrl, setCoverUrl] = useState<string | null>(null)
  const coverObjectUrlRef = useRef<string | null>(null)
  const [coverDraft, setCoverDraft] = useState<SocialCoverDraftState | null>(null)
  const [coverDraftUrl, setCoverDraftUrl] = useState<string | null>(null)
  const coverDraftObjectUrlRef = useRef<string | null>(null)
  const [coverPublishState, setCoverPublishState] = useState<CoverPublishState>('savedLocally')
  const [coverPublishError, setCoverPublishError] = useState<string | null>(null)
  const [coverPublishBy, setCoverPublishBy] = useState<string | null>(null)
  const [profileEditorOpen, setProfileEditorOpen] = useState(false)
  const [drawerOpen, setDrawerOpenState] = useState(() => {
    const raw = typeof window !== 'undefined' ? safeLocalStorageGet(`${STORAGE_PREFIX}:drawer-open`) : null
    if (raw === 'true') return true
    if (raw === 'false') return false
    return typeof window !== 'undefined' ? window.innerWidth >= 1280 : true
  })
  const [drawerWidth, setDrawerWidthState] = useState(() => {
    const raw = typeof window !== 'undefined' ? Number(safeLocalStorageGet(`${STORAGE_PREFIX}:drawer-width`)) : WIDTH_DEFAULT
    return Number.isFinite(raw) && raw > 0 ? clampWidth(raw) : WIDTH_DEFAULT
  })
  const [search, setSearch] = useState('')
  const [profileUserId, setProfileUserId] = useState<string | null>(null)
  const [fullProfileUserId, setFullProfileUserId] = useState<string | null>(null)

  const applyCoverState = useCallback((state: SocialCoverState) => {
    setPublicCoverConfirmed(state.publicUploadConfirmed)
    setCoverPublishState(state.publishState)
    setCoverPublishError(state.lastError)
    setCoverPublishBy(state.publishBy)
  }, [])

  useEffect(() => {
    setMigrationDecision(safeLocalStorageGet(`${STORAGE_PREFIX}:profile-migration:${selfId}`))
  }, [selfId])

  useEffect(() => {
    setRemoteMembers((current) => upsertFallbackSelf(current, fallbackSelf, selfId))
  }, [fallbackSelf, selfId])

  const registerMembers = useCallback((discovered: SocialMember[]) => {
    if (!discovered.length) return
    setRemoteMembers((current) => {
      const merged = new Map(current.map((member) => [member.id, member]))
      for (const member of discovered) merged.set(member.id, member)
      return [...merged.values()]
    })
  }, [])

  const applyRustCoverPreview = useCallback(async () => {
    const bytes = await readSocialCoverPreview()
    if (!bytes.byteLength) {
      if (coverObjectUrlRef.current) URL.revokeObjectURL(coverObjectUrlRef.current)
      coverObjectUrlRef.current = null
      setCoverUrl(null)
      return false
    }
    const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'image/webp' }))
    if (coverObjectUrlRef.current) URL.revokeObjectURL(coverObjectUrlRef.current)
    coverObjectUrlRef.current = url
    setCoverUrl(url)
    return true
  }, [])

  const applyLocalProfileSnapshot = useCallback((snapshot: LocalSocialProfileSnapshot) => {
    // Cover bytes in the v1 profile store are migration input only. The Rust-managed
    // canonical cover is the sole render source after migration/commit.
    setLocalProfile(snapshot.profile)
  }, [])

  useEffect(() => {
    let cancelled = false
    void (async () => {
      const snapshot = await loadLocalSocialProfile(defaultLocalProfile)
      if (cancelled) return
      applyLocalProfileSnapshot(snapshot)
      try {
        let state = await getSocialCoverState()
        if (!state.localPath && !state.removed && snapshot.coverBytes?.byteLength && snapshot.profile.coverMime) {
          state = await migrateLegacySocialCover(snapshot.coverBytes, snapshot.profile.coverMime)
        }
        if (state.localPath || state.removed) {
          if (snapshot.profile.coverMime || snapshot.coverBytes?.byteLength) {
            const migrated = await saveLocalSocialProfile(
              { ...snapshot.profile, coverMime: undefined },
              defaultLocalProfile,
              { cover: null },
            )
            if (!cancelled) setLocalProfile(migrated.profile)
          }
          if (!cancelled) {
            applyCoverState(state)
            await applyRustCoverPreview()
          }
        }
      } catch (coverError) {
        if (!cancelled) setError(coverError instanceof Error ? coverError.message : String(coverError))
      } finally {
        if (!cancelled) setLocalProfileLoaded(true)
      }
    })()
    return () => { cancelled = true }
  }, [applyCoverState, applyLocalProfileSnapshot, applyRustCoverPreview, defaultLocalProfile])

  useEffect(() => () => {
    if (coverObjectUrlRef.current) URL.revokeObjectURL(coverObjectUrlRef.current)
    if (coverDraftObjectUrlRef.current) URL.revokeObjectURL(coverDraftObjectUrlRef.current)
  }, [])

  const refreshSocial = useCallback(async () => {
    if (!user) {
      setRemoteMembers([fallbackSelf])
      publishSocialOnlineCount(0)
      setLoading(false)
      return
    }
    try {
      const bootstrap = await getSocialBootstrap()
      const self = profileDtoToMember(bootstrap.selfProfile)
      const next = [self, ...bootstrap.members.map(profileDtoToMember)]
      setServerSelfProfile(self)
      setRemoteMembers(next)
      publishSocialOnlineCount(next.filter((member) => member.id !== selfId && member.presence !== 'offline').length)
      setError(bootstrap.offlineSnapshot ? 'Offline snapshot' : null)
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError))
      setRemoteMembers((current) => upsertFallbackSelf(current, fallbackSelf, selfId)
        .map((member) => ({ ...member, presence: member.id === selfId ? 'online' : 'offline' })))
      publishSocialOnlineCount(0)
    } finally {
      setLoading(false)
    }
  }, [fallbackSelf, selfId, user])

  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | undefined
    setLoading(Boolean(user))
    void refreshSocial()
    if (user) {
      void listenForSocialEvents((event: SocialServerEvent) => {
        if (cancelled) return
        if (event.type === 'social.resync') {
          void refreshSocial()
        } else if (event.type === 'presence.updated') {
          const userId = String(event.payload.userId || '')
          const presence = String(event.payload.presence || 'offline')
          const activity = event.payload.activity as { kind?: string; label?: string; detail?: string; sinceMs?: number } | undefined
          setRemoteMembers((current) => current.map((member) => member.id !== userId ? member : {
            ...member,
            presence: presence === 'idle' ? 'idle' : presence === 'offline' ? 'offline' : 'online',
            activity: activity ? {
              kind: activity.kind === 'game' ? 'game' : activity.kind === 'none' ? 'none' : 'launcher',
              label: activity.label || member.activity.label,
              detail: activity.detail || '',
              minutes: activity.sinceMs ? Math.max(0, Math.round((Date.now() - activity.sinceMs) / 60_000)) : undefined,
            } : member.activity,
          }))
        } else if (event.type === 'profile.updated' && event.payload.profile) {
          const updated = profileDtoToMember(event.payload.profile as never)
          if (updated.id === selfId) setServerSelfProfile(updated)
          setRemoteMembers((current) => current.map((member) => member.id === updated.id ? updated : member))
        } else if (event.type === 'relationship.updated' || event.type === 'cover.published' || event.type === 'cover.removed') {
          void refreshSocial()
          if (event.type.startsWith('cover.')) {
            void getSocialCoverState().then((state) => {
              applyCoverState(state)
              if (state.removed || state.localPath) void applyRustCoverPreview()
            }).catch(() => undefined)
          }
        }
      }).then((stop) => { unlisten = stop })
    }
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [applyCoverState, applyRustCoverPreview, refreshSocial, selfId, user])

  // The native retry worker can succeed or fail while the Social UI is closed. Listen for
  // its authoritative state so the banner never remains stale after a background retry.
  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | undefined
    void listenForSocialCoverState((state) => {
      if (cancelled) return
      applyCoverState(state)
      if (state.removed || state.localPath) void applyRustCoverPreview()
    }).then((stop) => { unlisten = stop }).catch(() => undefined)
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [applyCoverState, applyRustCoverPreview])

  useEffect(() => {
    if (!user) return
    let cancelled = false
    void getSocialCoverState().then(async (state) => {
      if (cancelled) return
      applyCoverState(state)
      if (state.localPath) await applyRustCoverPreview()
    }).catch(() => undefined)
    return () => { cancelled = true }
  }, [applyCoverState, applyRustCoverPreview, user])

  const migrationPending = useMemo(() => {
    if (!user || !localProfileLoaded || !serverSelfProfile || migrationDecision) return false
    const serverIsEmpty = !serverSelfProfile.bio.trim()
      && !serverSelfProfile.activity.detail?.trim()
      && !serverSelfProfile.coverHash
    const localHasContent = Boolean(localProfile.bio.trim() || localProfile.customStatus.trim() || localProfile.coverMime)
    return serverIsEmpty && localHasContent
  }, [localProfile, localProfileLoaded, migrationDecision, serverSelfProfile, user])

  const members = useMemo(
    () => remoteMembers.map((member) => ({
      ...member,
      ...(member.id === selfId ? {
        bio: localProfile.bio || member.bio,
        activity: {
          ...member.activity,
          detail: localProfile.customStatus || member.activity.detail,
        },
      } : {}),
    })),
    [localProfile, remoteMembers, selfId],
  )

  useEffect(() => {
    publishSocialOnlineCount(members.filter((member) => member.id !== selfId && member.presence !== 'offline').length)
  }, [members, selfId])

  useEffect(() => {
    if (!user) return
    const sessionId = crypto.randomUUID()
    const sinceMs = Date.now()
    let stopped = false
    const heartbeat = () => {
      if (stopped) return
      const state = activeGame ? 'playing' : document.visibilityState === 'hidden' ? 'idle' : 'online'
      void updateSocialPresence({
        sessionId,
        state,
        activityLabel: activeGame?.label || (state === 'idle' ? 'Away' : 'Using 0xoLemon'),
        activityDetail: activeGame ? 'Playing now' : state === 'idle' ? 'Launcher is in the background' : '',
        gameId: activeGame?.gameId || null,
        sinceMs,
      }).catch((presenceError) => {
        if (!stopped) setError(presenceError instanceof Error ? presenceError.message : String(presenceError))
      })
    }
    heartbeat()
    const interval = window.setInterval(heartbeat, 45_000)
    document.addEventListener('visibilitychange', heartbeat)
    return () => {
      stopped = true
      window.clearInterval(interval)
      document.removeEventListener('visibilitychange', heartbeat)
      void updateSocialPresence({
        sessionId,
        state: 'offline',
        activityLabel: 'Offline',
        activityDetail: '',
        sinceMs,
      }).catch(() => undefined)
    }
  }, [activeGame, user])

  useEffect(() => {
    if (!user) return
    const interval = window.setInterval(() => {
      void recordSocialStats({
        eventId: crypto.randomUUID(),
        kind: 'heartbeat',
        onlineMinutes: 15,
      }).catch(() => undefined)
    }, 15 * 60_000)
    return () => window.clearInterval(interval)
  }, [user])

  const setDrawerOpen = useCallback((open: boolean) => {
    setDrawerOpenState(open)
    safeLocalStorageSet(`${STORAGE_PREFIX}:drawer-open`, String(open))
  }, [])
  const setDrawerWidth = useCallback((width: number) => {
    const next = clampWidth(width)
    setDrawerWidthState(next)
    safeLocalStorageSet(`${STORAGE_PREFIX}:drawer-width`, String(next))
  }, [])
  const openProfile = useCallback((userId: string) => {
    setProfileUserId(userId)
    setFullProfileUserId(null)
  }, [])
  const openFullProfile = useCallback((userId: string) => {
    setFullProfileUserId(userId)
    setProfileUserId(null)
  }, [])
  const closeProfile = useCallback(() => {
    setProfileUserId(null)
    setFullProfileUserId(null)
  }, [])
  const openProfileEditor = useCallback(() => setProfileEditorOpen(true), [])
  const closeProfileEditor = useCallback(() => setProfileEditorOpen(false), [])

  const updateRelationship = useCallback((userId: string, relationship: SocialRelationship) => {
    if (userId === selfId) return
    const previous = remoteMembers.find((member) => member.id === userId)?.relationship ?? 'none'
    setRemoteMembers((current) => current.map((member) => member.id === userId ? { ...member, relationship } : member))
    void updateSocialRelationship(userId, previous, relationship).catch((requestError) => {
      setRemoteMembers((current) => current.map((member) => member.id === userId ? { ...member, relationship: previous } : member))
      setError(requestError instanceof Error ? requestError.message : String(requestError))
    })
  }, [remoteMembers, selfId])

  const clearCoverDraftPreview = useCallback(() => {
    if (coverDraftObjectUrlRef.current) URL.revokeObjectURL(coverDraftObjectUrlRef.current)
    coverDraftObjectUrlRef.current = null
    setCoverDraftUrl(null)
    setCoverDraft(null)
  }, [])

  const discardSelfCoverDraft = useCallback(async () => {
    const activeDraft = coverDraft
    clearCoverDraftPreview()
    if (activeDraft) await discardSocialProfileDraft(activeDraft.draftId)
  }, [clearCoverDraftPreview, coverDraft])

  const saveSelfProfile = useCallback(async (
    profile: LocalSocialProfileDocument,
    cover: SocialProfileCoverOperation,
  ) => {
    const currentSelf = remoteMembers.find((member) => member.id === selfId) ?? fallbackSelf
    if (cover.operation === 'replace' && !coverDraft) {
      throw new Error('COVER_DRAFT_NOT_FOUND: Select the cover again.')
    }
    const coverState = await commitSocialProfileDraft({
      draftId: coverDraft?.draftId ?? crypto.randomUUID(),
      bio: profile.bio,
      status: profile.customStatus,
      accent: currentSelf.accent ?? '42 88% 58%',
      coverPosition: profile.coverPosition,
      cover,
    })
    let snapshot: LocalSocialProfileSnapshot
    try {
      snapshot = await saveLocalSocialProfile(
        { ...profile, coverMime: undefined },
        defaultLocalProfile,
        { cover: null },
      )
    } catch (legacyStoreError) {
      // Rust has already durably committed the canonical v2 profile. A failure while
      // retiring the v1 compatibility file must not turn a successful cover commit
      // into a retry against a draft that no longer exists.
      snapshot = {
        profile: {
          ...profile,
          version: 1,
          discordId: defaultLocalProfile.discordId,
          coverMime: undefined,
          updatedAt: new Date().toISOString(),
        },
      }
      setError(legacyStoreError instanceof Error ? legacyStoreError.message : String(legacyStoreError))
    }
    setLocalProfile(snapshot.profile)
    applyCoverState(coverState)
    clearCoverDraftPreview()
    await applyRustCoverPreview()
    setRemoteMembers((current) => upsertFallbackSelf(current.map((member) => member.id === selfId ? {
      ...member,
      bio: snapshot.profile.bio,
      activity: {
        ...member.activity,
        detail: snapshot.profile.customStatus,
      },
    } : member), fallbackSelf, selfId))

    try {
      const updated = await updateSocialProfile({
        bio: snapshot.profile.bio,
        customStatus: snapshot.profile.customStatus,
        accent: currentSelf.accent ?? '42 88% 58%',
        coverPosition: snapshot.profile.coverPosition,
        appearOffline: currentSelf.appearOffline === true,
      })
      const member = profileDtoToMember(updated)
      setRemoteMembers((current) => current.map((entry) => entry.id === member.id ? member : entry))
      setError(null)
    } catch (requestError) {
      // Profile editing is local-first. A backend outage must not discard a user's saved profile.
      setError(requestError instanceof Error ? requestError.message : String(requestError))
    }
    return coverState.publishState
  }, [applyCoverState, applyRustCoverPreview, clearCoverDraftPreview, coverDraft, defaultLocalProfile, fallbackSelf, remoteMembers, selfId])

  const chooseSelfCover = useCallback(async (publicUploadConfirmed: boolean) => {
    const staged = await stageSocialCover(publicUploadConfirmed)
    if (!staged) return null
    const bytes = await readSocialCoverDraftPreview(staged.draftId)
    const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: staged.mime }))
    const previousDraft = coverDraft
    if (coverDraftObjectUrlRef.current) URL.revokeObjectURL(coverDraftObjectUrlRef.current)
    coverDraftObjectUrlRef.current = url
    setCoverDraftUrl(url)
    setCoverDraft(staged)
    setPublicCoverConfirmed(staged.publicUploadConfirmed)
    if (previousDraft) {
      await discardSocialProfileDraft(previousDraft.draftId).catch(() => undefined)
    }
    return staged
  }, [coverDraft])
  const retrySelfCoverPublish = useCallback(async () => {
    const state = await retrySocialCoverPublish()
    applyCoverState(state)
    return state.publishState
  }, [applyCoverState])
  const setLeaderboardParticipation = useCallback(async (enabled: boolean) => {
    await setSocialLeaderboardParticipation(enabled)
    setRemoteMembers((current) => current.map((member) => member.id === selfId
      ? { ...member, leaderboardOptIn: enabled }
      : member))
  }, [selfId])
  const setAppearOffline = useCallback(async (enabled: boolean) => {
    const self = remoteMembers.find((member) => member.id === selfId)
    const updated = await updateSocialProfile({
      bio: self?.bio || localProfile.bio,
      customStatus: localProfile.customStatus,
      accent: self?.accent || '42 88% 58%',
      coverPosition: localProfile.coverPosition,
      appearOffline: enabled,
    })
    const member = profileDtoToMember(updated)
    setRemoteMembers((current) => current.map((entry) => entry.id === member.id ? member : entry))
  }, [localProfile, remoteMembers, selfId])
  const migrateLocalProfile = useCallback(async () => {
    if (!migrationPending) return
    const self = remoteMembers.find((member) => member.id === selfId)
    const updated = await updateSocialProfile({
      bio: localProfile.bio,
      customStatus: localProfile.customStatus,
      accent: self?.accent || '42 88% 58%',
      coverPosition: localProfile.coverPosition,
      appearOffline: self?.appearOffline === true,
    })
    const member = profileDtoToMember(updated)
    setRemoteMembers((current) => current.map((entry) => entry.id === member.id ? member : entry))
    setServerSelfProfile(member)
    const key = `${STORAGE_PREFIX}:profile-migration:${selfId}`
    safeLocalStorageSet(key, 'migrated')
    setMigrationDecision('migrated')
  }, [localProfile, migrationPending, remoteMembers, selfId])
  const dismissLocalProfileMigration = useCallback(() => {
    const key = `${STORAGE_PREFIX}:profile-migration:${selfId}`
    safeLocalStorageSet(key, 'dismissed')
    setMigrationDecision('dismissed')
  }, [selfId])
  const restoreSelfProfile = useCallback(async () => {
    const snapshot = await restorePreviousLocalSocialProfile(defaultLocalProfile)
    if (!snapshot) return false
    applyLocalProfileSnapshot(snapshot)
    return true
  }, [applyLocalProfileSnapshot, defaultLocalProfile])

  const value = useMemo<SocialPrototypeContextValue>(() => ({
    selfId,
    members,
    drawerOpen,
    drawerWidth,
    search,
    profileUserId,
    fullProfileUserId,
    localProfile,
    coverUrl,
    coverDraft,
    coverDraftUrl,
    coverPublishState,
    coverPublishError,
    coverPublishBy,
    profileEditorOpen,
    loading,
    error,
    publicCoverConfirmed,
    migrationPending,
    setDrawerOpen,
    setDrawerWidth,
    setSearch,
    openProfile,
    openFullProfile,
    closeProfile,
    openProfileEditor,
    closeProfileEditor,
    saveSelfProfile,
    restoreSelfProfile,
    chooseSelfCover,
    discardSelfCoverDraft,
    retrySelfCoverPublish,
    registerMembers,
    setLeaderboardParticipation,
    setAppearOffline,
    migrateLocalProfile,
    dismissLocalProfileMigration,
    updateRelationship,
  }), [
    selfId, members, drawerOpen, drawerWidth, search, profileUserId, fullProfileUserId,
    localProfile, coverUrl, coverDraft, coverDraftUrl, coverPublishState, coverPublishError, coverPublishBy,
    profileEditorOpen, loading, error, publicCoverConfirmed,
    migrationPending, setDrawerOpen, setDrawerWidth, openProfile, openFullProfile,
    closeProfile, openProfileEditor, closeProfileEditor, saveSelfProfile, restoreSelfProfile,
    chooseSelfCover, discardSelfCoverDraft, retrySelfCoverPublish, registerMembers, setLeaderboardParticipation,
    setAppearOffline, migrateLocalProfile, dismissLocalProfileMigration, updateRelationship,
  ])

  return <SocialPrototypeContext.Provider value={value}>{children}</SocialPrototypeContext.Provider>
}
