import {
  useCallback,
  useContext,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  type PropsWithChildren,
} from 'react'
import { AnimatePresence, motion } from 'motion/react'
import {
  Activity,
  BarChart3,
  BellDot,
  Check,
  ChevronDown,
  ChevronRight,
  Clock3,
  Copy,
  Download,
  Gamepad2,
  ImagePlus,
  Medal,
  MoreHorizontal,
  Pencil,
  RotateCcw,
  Save,
  Search,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  Trophy,
  UserCheck,
  UserPlus,
  UsersRound,
  X,
} from 'lucide-react'
import type { DiscordAuthUser } from '../types'
import { useLocale } from '../context/locale'
import {
  formatSocialMinutes,
  type SocialLeaderboardMetric,
  type SocialMember,
  type SocialRelationship,
} from './socialModels'
import {
  chooseSocialCover,
  getSocialBootstrap,
  getSocialCoverState,
  getSocialLeaderboard,
  listenForSocialEvents,
  profileDtoToMember,
  publishSocialOnlineCount,
  readSocialCoverPreview,
  recordSocialStats,
  removeSocialCover,
  searchSocialUsers,
  setSocialLeaderboardParticipation,
  updateSocialProfile,
  updateSocialPresence,
  updateSocialRelationship,
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
import { SocialPrototypeContext } from './SocialProvider'
import './SocialPrototype.css'

type SocialPrototypeContextValue = {
  selfId: string
  members: SocialMember[]
  drawerOpen: boolean
  drawerWidth: number
  search: string
  profileUserId: string | null
  fullProfileUserId: string | null
  localProfile: LocalSocialProfileDocument
  coverUrl: string | null
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
  saveSelfProfile: (profile: LocalSocialProfileDocument) => Promise<void>
  restoreSelfProfile: () => Promise<boolean>
  chooseSelfCover: (publicUploadConfirmed: boolean) => Promise<boolean>
  removeSelfCover: () => Promise<void>
  registerMembers: (members: SocialMember[]) => void
  setLeaderboardParticipation: (enabled: boolean) => Promise<void>
  setAppearOffline: (enabled: boolean) => Promise<void>
  migrateLocalProfile: () => Promise<void>
  dismissLocalProfileMigration: () => void
  updateRelationship: (userId: string, relationship: SocialRelationship) => void
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
    // Drawer preferences are optional. Ignore blocked storage contexts.
  }
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

  useEffect(() => {
    setMigrationDecision(safeLocalStorageGet(`${STORAGE_PREFIX}:profile-migration:${selfId}`))
  }, [selfId])

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
    setLocalProfile(snapshot.profile)
    if (coverObjectUrlRef.current) {
      URL.revokeObjectURL(coverObjectUrlRef.current)
      coverObjectUrlRef.current = null
    }
    if (snapshot.coverBytes?.byteLength && snapshot.profile.coverMime) {
      // Tauri's readFile() is typed as Uint8Array<ArrayBufferLike>. BlobPart only accepts
      // ArrayBuffer-backed views, so clone the bytes into a fresh non-shared ArrayBuffer.
      const coverBytesForBlob = new Uint8Array(snapshot.coverBytes)
      const url = URL.createObjectURL(new Blob([coverBytesForBlob], { type: snapshot.profile.coverMime }))
      coverObjectUrlRef.current = url
      setCoverUrl(url)
    } else {
      setCoverUrl(null)
    }
  }, [])

  useEffect(() => {
    let cancelled = false
    void loadLocalSocialProfile(defaultLocalProfile).then((snapshot) => {
      if (!cancelled) {
        applyLocalProfileSnapshot(snapshot)
        setLocalProfileLoaded(true)
      }
    })
    return () => {
      cancelled = true
    }
  }, [applyLocalProfileSnapshot, defaultLocalProfile])

  useEffect(() => () => {
    if (coverObjectUrlRef.current) URL.revokeObjectURL(coverObjectUrlRef.current)
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
      const next = [bootstrap.selfProfile, ...bootstrap.members].map(profileDtoToMember)
      setServerSelfProfile(profileDtoToMember(bootstrap.selfProfile))
      setRemoteMembers(next)
      publishSocialOnlineCount(next.filter((member) => member.id !== selfId && member.presence !== 'offline').length)
      setError(bootstrap.offlineSnapshot ? 'Offline snapshot' : null)
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError))
      setRemoteMembers((current) => current.length ? current.map((member) => ({ ...member, presence: member.id === selfId ? 'online' : 'offline' })) : [fallbackSelf])
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
          setRemoteMembers((current) => current.map((member) => member.id === updated.id ? updated : member))
        } else if (event.type === 'relationship.updated' || event.type === 'cover.published' || event.type === 'cover.removed') {
          void refreshSocial()
        }
      }).then((stop) => { unlisten = stop })
    }
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [refreshSocial, user])

  useEffect(() => {
    if (!user) return
    let cancelled = false
    void getSocialCoverState().then(async (state) => {
      if (cancelled) return
      setPublicCoverConfirmed(state.publicUploadConfirmed)
      if (state.localPath) await applyRustCoverPreview()
    }).catch(() => undefined)
    return () => { cancelled = true }
  }, [applyRustCoverPreview, user])

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
        bio: localProfile.bio,
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

  const updateRelationship = useCallback((userId: string, relationship: SocialRelationship) => {
    if (userId === selfId) return
    const previous = remoteMembers.find((member) => member.id === userId)?.relationship ?? 'none'
    setRemoteMembers((current) => current.map((member) => member.id === userId ? { ...member, relationship } : member))
    void updateSocialRelationship(userId, previous, relationship).catch((requestError) => {
      setRemoteMembers((current) => current.map((member) => member.id === userId ? { ...member, relationship: previous } : member))
      setError(requestError instanceof Error ? requestError.message : String(requestError))
    })
  }, [remoteMembers, selfId])

  const openProfileEditor = useCallback(() => setProfileEditorOpen(true), [])
  const closeProfileEditor = useCallback(() => setProfileEditorOpen(false), [])

  const saveSelfProfile = useCallback(async (profile: LocalSocialProfileDocument) => {
    const snapshot = await saveLocalSocialProfile(profile, defaultLocalProfile)
    if (snapshot.coverBytes?.byteLength) applyLocalProfileSnapshot(snapshot)
    else setLocalProfile(snapshot.profile)
    const updated = await updateSocialProfile({
      bio: snapshot.profile.bio,
      customStatus: snapshot.profile.customStatus,
      accent: remoteMembers.find((member) => member.id === selfId)?.accent ?? '42 88% 58%',
      coverPosition: snapshot.profile.coverPosition,
      appearOffline: remoteMembers.find((member) => member.id === selfId)?.appearOffline === true,
    })
    const member = profileDtoToMember(updated)
    setRemoteMembers((current) => current.map((entry) => entry.id === member.id ? member : entry))
  }, [applyLocalProfileSnapshot, defaultLocalProfile, remoteMembers, selfId])

  const chooseSelfCover = useCallback(async (publicUploadConfirmed: boolean) => {
    const state = await chooseSocialCover(publicUploadConfirmed)
    if (!state) return false
    setPublicCoverConfirmed(state.publicUploadConfirmed)
    await applyRustCoverPreview()
    return true
  }, [applyRustCoverPreview])

  const removeSelfCover = useCallback(async () => {
    await removeSocialCover()
    await applyRustCoverPreview()
  }, [applyRustCoverPreview])

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
    removeSelfCover,
    registerMembers,
    setLeaderboardParticipation,
    setAppearOffline,
    migrateLocalProfile,
    dismissLocalProfileMigration,
    updateRelationship,
  }), [
    selfId,
    members,
    drawerOpen,
    drawerWidth,
    search,
    profileUserId,
    fullProfileUserId,
    localProfile,
    coverUrl,
    profileEditorOpen,
    loading,
    error,
    publicCoverConfirmed,
    migrationPending,
    setDrawerOpen,
    setDrawerWidth,
    openProfile,
    openFullProfile,
    closeProfile,
    openProfileEditor,
    closeProfileEditor,
    saveSelfProfile,
    restoreSelfProfile,
    chooseSelfCover,
    removeSelfCover,
    registerMembers,
    setLeaderboardParticipation,
    setAppearOffline,
    migrateLocalProfile,
    dismissLocalProfileMigration,
    updateRelationship,
  ])

  // Kept only as a compatibility export. App.tsx mounts the canonical provider from
  // SocialProvider.tsx; this legacy implementation is not on the production call graph.
  return <SocialPrototypeContext.Provider value={value as never}>{children}</SocialPrototypeContext.Provider>
}

function useSocialPrototype() {
  const value = useContext(SocialPrototypeContext)
  if (!value) throw new Error('Social components must be inside SocialPrototypeProvider')
  return value
}

function Avatar({ member, size = 38 }: { member: SocialMember; size?: number }) {
  const initials = member.displayName.trim().slice(0, 2).toUpperCase()
  return (
    <span
      className="social-avatar"
      style={{ width: size, height: size, '--social-avatar-accent': `hsl(${member.accent})` } as CSSProperties}
      aria-hidden="true"
    >
      {member.avatarUrl ? <img src={member.avatarUrl} alt="" /> : <span>{initials}</span>}
      <i className={`social-presence social-presence-${member.presence}`} />
    </span>
  )
}

type SocialStrings = ReturnType<typeof useLocale>['t']['social']

function relationshipLabel(relationship: SocialRelationship, labels: SocialStrings) {
  if (relationship === 'friend') return labels.friends
  if (relationship === 'pending-in') return labels.requestReceived
  if (relationship === 'pending-out') return labels.requestSent
  if (relationship === 'blocked') return labels.blocked
  return labels.notFriends
}

function profileBannerStyle(
  member: SocialMember,
  selfId: string,
  coverUrl: string | null,
  localProfile: LocalSocialProfileDocument,
): CSSProperties {
  const style: CSSProperties = {
    '--social-profile-accent': `hsl(${member.accent})`,
  } as CSSProperties
  const effectiveCover = member.id === selfId && coverUrl ? coverUrl : member.coverUrl
  if (effectiveCover) {
    style.backgroundImage = `linear-gradient(180deg, rgba(5, 8, 12, .08), rgba(5, 8, 12, .34)), url("${effectiveCover}")`
    style.backgroundPosition = `center ${member.id === selfId ? localProfile.coverPosition : member.coverPosition ?? 50}%`
  }
  return style
}

function MemberRow({ member, compact = false }: { member: SocialMember; compact?: boolean }) {
  const { t } = useLocale()
  const { openProfile } = useSocialPrototype()
  return (
    <button
      type="button"
      className={`social-member-row${compact ? ' is-compact' : ''}`}
      onClick={() => openProfile(member.id)}
      title={`${t.social.viewProfile}: ${member.displayName}`}
    >
      <Avatar member={member} size={compact ? 34 : 38} />
      <span className="social-member-copy">
        <strong>{member.displayName}</strong>
        <small>
          {member.activity.kind === 'game' ? `${t.social.playing} ${member.activity.label}` : member.activity.label}
          {member.activity.minutes ? ` · ${member.activity.minutes}m` : ''}
        </small>
      </span>
      {member.activity.kind === 'game' ? <Gamepad2 size={15} className="social-member-game" /> : null}
    </button>
  )
}

function Section({
  label,
  members,
  defaultOpen = true,
}: {
  label: string
  members: SocialMember[]
  defaultOpen?: boolean
}) {
  const [open, setOpen] = useState(defaultOpen)
  if (members.length === 0) return null
  return (
    <section className="social-member-section">
      <button type="button" className="social-section-label" onClick={() => setOpen((value) => !value)}>
        {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        <span>{label} — {members.length}</span>
      </button>
      {open ? <div className="social-section-list">{members.map((member) => <MemberRow key={member.id} member={member} />)}</div> : null}
    </section>
  )
}

function useResizeDrawer() {
  const { drawerWidth, setDrawerWidth } = useSocialPrototype()
  const dragRef = useRef<{ startX: number; startWidth: number } | null>(null)

  useEffect(() => {
    const move = (event: MouseEvent) => {
      const drag = dragRef.current
      if (!drag) return
      setDrawerWidth(drag.startWidth + (drag.startX - event.clientX))
    }
    const up = () => {
      dragRef.current = null
      document.body.classList.remove('social-resizing')
    }
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
    return () => {
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
    }
  }, [setDrawerWidth])

  return {
    onMouseDown: (event: ReactMouseEvent) => {
      event.preventDefault()
      dragRef.current = { startX: event.clientX, startWidth: drawerWidth }
      document.body.classList.add('social-resizing')
    },
    onDoubleClick: () => setDrawerWidth(WIDTH_DEFAULT),
  }
}

function ProfileActions({ member }: { member: SocialMember }) {
  const { t } = useLocale()
  const { selfId, updateRelationship, openFullProfile } = useSocialPrototype()
  if (member.id === selfId) {
    return (
      <div className="social-profile-actions">
        <button type="button" className="social-primary" onClick={() => openFullProfile(member.id)}>{t.social.viewMyProfile}</button>
      </div>
    )
  }
  if (member.relationship === 'pending-in') {
    return (
      <div className="social-profile-actions social-profile-actions-split">
        <button type="button" className="social-primary" onClick={() => updateRelationship(member.id, 'friend')}><Check size={15} /> {t.social.accept}</button>
        <button type="button" className="social-secondary" onClick={() => updateRelationship(member.id, 'none')}>{t.social.ignore}</button>
      </div>
    )
  }
  if (member.relationship === 'pending-out') {
    return (
      <div className="social-profile-actions social-profile-actions-split">
        <button type="button" className="social-secondary" disabled>{t.social.requestSent}</button>
        <button type="button" className="social-secondary" onClick={() => updateRelationship(member.id, 'none')}>{t.social.cancel}</button>
      </div>
    )
  }
  if (member.relationship === 'friend') {
    return (
      <div className="social-profile-actions social-profile-actions-split">
        <button type="button" className="social-primary" onClick={() => openFullProfile(member.id)}>{t.social.viewProfile}</button>
        <button type="button" className="social-secondary" onClick={() => updateRelationship(member.id, 'none')}>{t.social.removeFriend}</button>
      </div>
    )
  }
  return (
    <div className="social-profile-actions social-profile-actions-split">
      <button type="button" className="social-primary" onClick={() => updateRelationship(member.id, 'pending-out')}><UserPlus size={15} /> {t.social.addFriendAction}</button>
      <button type="button" className="social-secondary" onClick={() => openFullProfile(member.id)}>{t.social.viewProfile}</button>
    </div>
  )
}

function ProfilePopout() {
  const { t } = useLocale()
  const { members, profileUserId, closeProfile, selfId, coverUrl, localProfile } = useSocialPrototype()
  const member = members.find((item) => item.id === profileUserId)
  return (
    <AnimatePresence>
      {member ? (
        <motion.div
          className="social-profile-popout"
          initial={{ opacity: 0, x: 18, scale: 0.98 }}
          animate={{ opacity: 1, x: 0, scale: 1 }}
          exit={{ opacity: 0, x: 12, scale: 0.985 }}
          transition={{ duration: 0.18, ease: [0.2, 0, 0, 1] }}
        >
          <div
            className={`social-profile-banner${(member.id === selfId ? coverUrl : member.coverUrl) ? ' has-cover' : ''}`}
            style={profileBannerStyle(member, selfId, coverUrl, localProfile)}
          />
          <button type="button" className="social-profile-close" onClick={closeProfile} aria-label={t.whatsNew.close}><X size={15} /></button>
          <div className="social-profile-body">
            <Avatar member={member} size={72} />
            <div className="social-profile-name">
              <h3>{member.displayName}</h3>
              <span>@{member.username}</span>
              <small>{relationshipLabel(member.relationship, t.social)}</small>
            </div>
            <div className="social-profile-activity">
              {member.activity.kind === 'game' ? <Gamepad2 size={16} /> : <Activity size={16} />}
              <span><strong>{member.activity.label}</strong><small>{member.activity.detail}</small></span>
            </div>
            <p>{member.bio}</p>
            <div className="social-profile-mini-stats">
              <span><strong>{member.stats.downloads}</strong><small>{t.social.downloads}</small></span>
              <span><strong>{formatSocialMinutes(member.stats.playMinutes)}</strong><small>{t.social.playtime}</small></span>
              <span><strong>{formatSocialMinutes(member.stats.onlineMinutes)}</strong><small>{t.social.online}</small></span>
            </div>
            <ProfileActions member={member} />
          </div>
        </motion.div>
      ) : null}
    </AnimatePresence>
  )
}

function copyText(value: string) {
  if (navigator.clipboard?.writeText) return navigator.clipboard.writeText(value)
  const input = document.createElement('textarea')
  input.value = value
  document.body.appendChild(input)
  input.select()
  document.execCommand('copy')
  input.remove()
  return Promise.resolve()
}

function FullProfileModal() {
  const { t } = useLocale()
  const {
    members,
    fullProfileUserId,
    closeProfile,
    selfId,
    updateRelationship,
    coverUrl,
    localProfile,
    openProfileEditor,
  } = useSocialPrototype()
  const member = members.find((item) => item.id === fullProfileUserId)
  const [copied, setCopied] = useState(false)
  if (!member) return null

  const doCopy = () => {
    void copyText(member.id).then(() => {
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1300)
    })
  }

  return (
    <div className="social-full-profile-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.currentTarget === event.target) closeProfile()
    }}>
      <motion.section
        className="social-full-profile"
        role="dialog"
        aria-modal="true"
        aria-label={`${t.social.viewProfile}: ${member.displayName}`}
        initial={{ opacity: 0, y: 18, scale: 0.985 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        transition={{ duration: 0.22, ease: [0.2, 0, 0, 1] }}
      >
        <div
          className={`social-full-profile-banner${(member.id === selfId ? coverUrl : member.coverUrl) ? ' has-cover' : ''}`}
          style={profileBannerStyle(member, selfId, coverUrl, localProfile)}
        >
          <div className="social-full-profile-banner-glow" />
        </div>
        <button type="button" className="social-full-profile-close" onClick={closeProfile} aria-label={t.whatsNew.close}><X size={18} /></button>
        <div className="social-full-profile-main">
          <div className="social-full-profile-identity">
            <Avatar member={member} size={104} />
            <div>
              <h2>{member.displayName}</h2>
              <span>@{member.username}</span>
              <div className="social-identity-row">
                <code>{member.id}</code>
                <button type="button" onClick={doCopy}>{copied ? <Check size={14} /> : <Copy size={14} />}{copied ? t.social.copied : t.social.copyId}</button>
              </div>
            </div>
            {member.id !== selfId ? (
              <button
                type="button"
                className="social-primary social-full-profile-friend"
                onClick={() => updateRelationship(member.id, member.relationship === 'friend' ? 'none' : 'pending-out')}
              >
                {member.relationship === 'friend' ? <UserCheck size={16} /> : <UserPlus size={16} />}
                {member.relationship === 'friend' ? t.social.friends : member.relationship === 'pending-out' ? t.social.requestSent : t.social.addFriendAction}
              </button>
            ) : (
              <div className="social-profile-self-actions">
                <span className="social-profile-self-badge"><ShieldCheck size={15} /> {t.social.yourProfile}</span>
                <button type="button" className="social-secondary" onClick={openProfileEditor}><Pencil size={14} /> {t.social.editProfile}</button>
              </div>
            )}
          </div>

          <div className="social-full-profile-grid">
            <article className="social-full-profile-card social-about-card">
              <span className="social-card-kicker">{t.social.aboutMe}</span>
              <p>{member.bio}</p>
              <div className="social-badge-row">
                {member.badges.map((badge) => <span key={badge}><Sparkles size={13} />{badge}</span>)}
              </div>
            </article>
            <article className="social-full-profile-card social-activity-card">
              <span className="social-card-kicker">{t.social.currentActivity}</span>
              <div className="social-profile-activity is-large">
                {member.activity.kind === 'game' ? <Gamepad2 size={20} /> : <Activity size={20} />}
                <span><strong>{member.activity.label}</strong><small>{member.activity.detail ?? t.social.noPublicActivity}</small></span>
              </div>
            </article>
          </div>

          <div className="social-stat-grid">
            <article><Download size={18} /><strong>{member.stats.downloads}</strong><span>{t.social.downloads}</span><small>{member.stats.downloadedGb.toLocaleString()} GB</small></article>
            <article><Gamepad2 size={18} /><strong>{formatSocialMinutes(member.stats.playMinutes)}</strong><span>{t.social.playtime}</span><small>{member.stats.gamesPlayed} {t.social.games.toLowerCase()}</small></article>
            <article><Clock3 size={18} /><strong>{formatSocialMinutes(member.stats.onlineMinutes)}</strong><span>{t.social.online}</span><small>0xoLemon</small></article>
          </div>

          <article className="social-full-profile-card social-recent-card">
            <span className="social-card-kicker">{t.social.recentlyPlayed}</span>
            <div className="social-recent-games">
              {member.recentGames.map((game, index) => (
                <div key={game} style={{ '--game-accent': `${(index * 74 + 28) % 360}` } as CSSProperties}>
                  <i><Gamepad2 size={18} /></i>
                  <span>{game}</span>
                </div>
              ))}
            </div>
          </article>
        </div>
      </motion.section>
    </div>
  )
}


function ProfileEditorModal() {
  const { t } = useLocale()
  const {
    members,
    selfId,
    profileEditorOpen,
    closeProfileEditor,
    localProfile,
    coverUrl,
    coverDraftUrl,
    saveSelfProfile,
    restoreSelfProfile,
    chooseSelfCover,
    discardSelfCoverDraft,
    publicCoverConfirmed,
    setAppearOffline,
  } = useSocialPrototype()
  const [draft, setDraft] = useState(localProfile)
  const [showReposition, setShowReposition] = useState(false)
  const [coverConsentChecked, setCoverConsentChecked] = useState(publicCoverConfirmed)
  const [appearOffline, setAppearOfflineDraft] = useState(false)
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  const [coverOperation, setCoverOperation] = useState<SocialProfileCoverOperation>({ operation: 'keep' })

  useEffect(() => {
    if (!profileEditorOpen) return
    setDraft(localProfile)
    setShowReposition(false)
    setCoverConsentChecked(publicCoverConfirmed)
    setAppearOfflineDraft(members.find((member) => member.id === selfId)?.appearOffline === true)
    setCoverOperation({ operation: 'keep' })
    setNotice(null)
  }, [localProfile, members, profileEditorOpen, publicCoverConfirmed, selfId])

  if (!profileEditorOpen) return null

  const previewUrl = coverOperation.operation === 'remove' ? null : coverDraftUrl ?? coverUrl

  const pickCover = async () => {
    setNotice(null)
    setBusy(true)
    setCoverConsentChecked(true)
    try {
      const selected = await chooseSelfCover(true)
      if (selected) {
        setCoverOperation({
          operation: 'replace',
          sha256: selected.sha256,
          mime: selected.mime,
          width: selected.width,
          height: selected.height,
        })
        setShowReposition(true)
        setNotice(t.social.coverSavedLocal)
      }
    } catch (error) {
      setNotice(error instanceof Error ? error.message : t.social.coverPrepareFailed)
    } finally {
      setBusy(false)
    }
  }

  const removeCover = async () => {
    setBusy(true)
    setNotice(null)
    try {
      await discardSelfCoverDraft()
      setCoverOperation({ operation: 'remove' })
      setShowReposition(false)
      setNotice(t.social.coverRemoved)
    } catch (error) {
      setNotice(error instanceof Error ? error.message : t.social.coverRemoveFailed)
    } finally {
      setBusy(false)
    }
  }

  const save = async () => {
    setBusy(true)
    setNotice(null)
    try {
      await saveSelfProfile(draft, coverOperation)
      const currentAppearOffline = members.find((member) => member.id === selfId)?.appearOffline === true
      if (currentAppearOffline !== appearOffline) await setAppearOffline(appearOffline)
      closeProfileEditor()
    } catch (error) {
      setNotice(error instanceof Error ? error.message : t.social.profileSaveFailed)
    } finally {
      setBusy(false)
    }
  }

  const cancel = async () => {
    if (busy) return
    setBusy(true)
    try {
      await discardSelfCoverDraft()
    } finally {
      setBusy(false)
      closeProfileEditor()
    }
  }

  const restore = async () => {
    setBusy(true)
    setNotice(null)
    try {
      const restored = await restoreSelfProfile()
      if (!restored) {
        setNotice(t.social.noPreviousBackup)
        return
      }
      setNotice(t.social.previousProfileRestored)
    } catch (error) {
      setNotice(error instanceof Error ? error.message : t.social.restoreFailed)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="social-profile-editor-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.currentTarget === event.target && !busy) void cancel()
    }}>
      <motion.section
        className="social-profile-editor"
        role="dialog"
        aria-modal="true"
        aria-label={t.social.customize}
        initial={{ opacity: 0, y: 18, scale: .985 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        transition={{ duration: .2, ease: [0.2, 0, 0, 1] }}
      >
        <header className="social-profile-editor-header">
          <div>
            <span className="social-card-kicker">{t.social.myProfile}</span>
            <h3>{t.social.profileEditorTitle}</h3>
            <p>{t.social.profileEditorDescription}</p>
          </div>
          <button type="button" className="social-icon-button" onClick={() => void cancel()} aria-label={t.whatsNew.close}><X size={20} strokeWidth={2.4} /></button>
        </header>

        <div className="social-cover-editor-preview">
          <div
            className={`social-cover-editor-image${previewUrl ? ' has-cover' : ''}`}
            style={previewUrl ? {
              backgroundImage: `linear-gradient(180deg, rgba(6,9,13,.05), rgba(6,9,13,.32)), url("${previewUrl}")`,
              backgroundPosition: `center ${draft.coverPosition}%`,
            } : undefined}
          >
            {!previewUrl ? <span><ImagePlus size={22} /> {t.social.chooseWideCover}</span> : null}
          </div>
          <div className="social-cover-editor-actions">
            <button type="button" className="social-secondary" onClick={() => void pickCover()} disabled={busy}><ImagePlus size={15} /> {previewUrl ? t.social.changeCover : t.social.addCover}</button>
            <button type="button" className="social-secondary" onClick={() => setShowReposition((value) => !value)} disabled={!previewUrl || busy}><SlidersHorizontal size={15} /> {t.social.reposition}</button>
            <button type="button" className="social-secondary is-danger-soft" onClick={() => void removeCover()} disabled={!previewUrl || busy}><Trash2 size={15} /> {t.social.remove}</button>
          </div>
          {!publicCoverConfirmed ? (
            <label className="social-cover-public-consent">
              <input type="checkbox" checked={coverConsentChecked} onChange={(event) => setCoverConsentChecked(event.target.checked)} />
              <span><strong>{t.social.publicCover}</strong>{t.social.publicCoverDescription}</span>
            </label>
          ) : (
            <div className="social-cover-public-consent is-confirmed"><ShieldCheck size={17} /><span><strong>{t.social.publicCoverEnabled}</strong>{t.social.publicCoverEnabledDescription}</span></div>
          )}
          {showReposition && previewUrl ? (
            <label className="social-cover-position-control">
              <span>{t.social.verticalCrop} <strong>{draft.coverPosition}%</strong></span>
              <input
                type="range"
                min="0"
                max="100"
                value={draft.coverPosition}
                onChange={(event) => setDraft((current) => ({ ...current, coverPosition: Number(event.target.value) }))}
              />
            </label>
          ) : null}
        </div>

        <div className="social-profile-editor-fields">
          <label>
            <span>{t.social.aboutMe} <small>{draft.bio.length}/480</small></span>
            <textarea
              value={draft.bio}
              maxLength={480}
              rows={4}
              onChange={(event) => setDraft((current) => ({ ...current, bio: event.target.value }))}
              placeholder={t.social.bioPlaceholder}
            />
          </label>
          <label>
            <span>{t.social.customStatus} <small>{draft.customStatus.length}/120</small></span>
            <input
              value={draft.customStatus}
              maxLength={120}
              onChange={(event) => setDraft((current) => ({ ...current, customStatus: event.target.value }))}
              placeholder={t.social.customStatusPlaceholder}
            />
          </label>
          <label className="social-appear-offline-control">
            <span><strong>{t.social.appearOffline}</strong><small>{t.social.appearOfflineDescription}</small></span>
            <input type="checkbox" checked={appearOffline} onChange={(event) => setAppearOfflineDraft(event.target.checked)} />
          </label>
        </div>

        {notice ? <div className="social-profile-editor-notice">{notice}</div> : null}

        <footer className="social-profile-editor-footer">
          <button type="button" className="social-secondary" onClick={() => void restore()} disabled={busy}><RotateCcw size={15} /> {t.social.restorePrevious}</button>
          <span>{t.social.rollingBackup}</span>
          <button type="button" className="social-secondary" onClick={() => void cancel()} disabled={busy}>{t.social.cancel}</button>
          <button type="button" className="social-primary" onClick={() => void save()} disabled={busy}><Save size={15} /> {busy ? t.social.saving : t.social.saveProfile}</button>
        </footer>
      </motion.section>
    </div>
  )
}

export function SocialPrototypeLayer({
  reducedMotion,
  onOpenSocial,
}: {
  reducedMotion: boolean
  onOpenSocial: () => void
}) {
  const { t } = useLocale()
  const {
    selfId,
    members,
    drawerOpen,
    drawerWidth,
    search,
    coverPublishState,
    coverPublishError,
    coverPublishBy,
    retrySelfCoverPublish,
    setDrawerOpen,
    setSearch,
    openProfile,
  } = useSocialPrototype()
  const resize = useResizeDrawer()
  const [coverPublishRetryBusy, setCoverPublishRetryBusy] = useState(false)
  const [coverBannerDismissed, setCoverBannerDismissed] = useState(false)
  const [layerVisible, setLayerVisibleState] = useState(() => {
    const raw = typeof window !== 'undefined' ? safeLocalStorageGet(`${STORAGE_PREFIX}:layer-visible`) : null
    if (raw === 'true') return true
    if (raw === 'false') return false
    return false
  })
  const setLayerVisible = useCallback((visible: boolean) => {
    setLayerVisibleState(visible)
    safeLocalStorageSet(`${STORAGE_PREFIX}:layer-visible`, String(visible))
    if (!visible) setDrawerOpen(false)
  }, [setDrawerOpen])
  const normalized = search.trim().toLowerCase()
  const visible = useMemo(() => members.filter((member) => member.id !== selfId).filter((member) => {
    if (!normalized) return true
    return `${member.displayName} ${member.username} ${member.id} ${member.activity.label}`.toLowerCase().includes(normalized)
  }), [members, selfId, normalized])

  const requests = visible.filter((member) => member.relationship === 'pending-in')
  const friends = visible.filter((member) => member.relationship === 'friend')
  const playing = friends.filter((member) => member.presence !== 'offline' && member.activity.kind === 'game')
  const playingIds = new Set(playing.map((member) => member.id))
  const online = friends.filter((member) => member.presence === 'online' && !playingIds.has(member.id))
  const idle = friends.filter((member) => member.presence === 'idle' && !playingIds.has(member.id))
  const offline = friends.filter((member) => member.presence === 'offline')
  const onlineCount = friends.filter((member) => member.presence !== 'offline').length + 1
  const coverPublishLabel = {
    savedLocally: t.social.coverPublishSavedLocally,
    pendingPublish: t.social.coverPublishPending,
    published: t.social.coverPublishPublished,
    publishFailed: t.social.coverPublishFailed,
  }[coverPublishState]

  useEffect(() => {
    const root = document.documentElement
    const drawerReserve = layerVisible && drawerOpen ? `${drawerWidth}px` : '0px'
    if (layerVisible) root.dataset.socialLayer = 'true'
    else delete root.dataset.socialLayer
    root.style.setProperty('--social-drawer-reserve', drawerReserve)
    // The entire social strip can be hidden, so reserve no workspace width at all
    // until the user explicitly opens it from the title bar or Social page.
    root.style.setProperty('--social-layout-reserve', layerVisible
      ? `calc(var(--social-rail-width, 54px) + ${drawerReserve})`
      : '0px')
    window.dispatchEvent(new CustomEvent('0xo-social-visibility-change', { detail: { visible: layerVisible } }))

    return () => {
      delete root.dataset.socialLayer
      root.style.removeProperty('--social-drawer-reserve')
      root.style.removeProperty('--social-layout-reserve')
    }
  }, [drawerOpen, drawerWidth, layerVisible])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && drawerOpen && window.innerWidth < 1450) setDrawerOpen(false)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [drawerOpen, setDrawerOpen])

  useEffect(() => {
    const toggle = () => {
      if (layerVisible) {
        setLayerVisible(false)
        return
      }
      setLayerVisible(true)
      setDrawerOpen(true)
    }
    window.addEventListener('0xo-social-toggle', toggle)
    return () => window.removeEventListener('0xo-social-toggle', toggle)
  }, [layerVisible, setDrawerOpen, setLayerVisible])

  return (
    <>
      <AnimatePresence initial={false}>
        {layerVisible ? (
          <motion.div
            key="social-global-layer"
            className={`social-global-layer${drawerOpen ? ' is-open' : ' is-collapsed'}`}
            style={{ '--social-drawer-width': `${drawerWidth}px` } as CSSProperties}
            initial={reducedMotion ? false : { opacity: 0, x: 18 }}
            animate={{ opacity: 1, x: 0 }}
            exit={reducedMotion ? { opacity: 0 } : { opacity: 0, x: 18 }}
            transition={reducedMotion ? { duration: 0 } : { duration: 0.36, ease: [0.16, 1, 0.3, 1] }}
          >
        <motion.aside
          className={`social-drawer${drawerOpen ? ' is-open' : ''}`}
          initial={reducedMotion ? false : { width: 0, opacity: 0 }}
          animate={{ width: drawerOpen ? drawerWidth : 0, opacity: drawerOpen ? 1 : 0 }}
          transition={reducedMotion ? { duration: 0 } : { duration: 0.36, ease: [0.16, 1, 0.3, 1] }}
          aria-hidden={!drawerOpen}
        >
          <button type="button" className="social-resize-handle" onMouseDown={resize.onMouseDown} onDoubleClick={resize.onDoubleClick} aria-label={t.social.resizeSocialPanel} />
          <header className="social-drawer-header">
            <div>
              <span>0xoLemon Social</span>
              <strong>{t.social.friends}</strong>
            </div>
            <div className="social-live-badge">{t.social.live}</div>
            <button type="button" className="social-icon-button" onClick={() => setLayerVisible(false)} aria-label={t.social.collapseSocialPanel} title={t.social.collapseSocialPanel}><ChevronRight size={20} strokeWidth={2.2} /></button>
          </header>
          <div className="social-search">
            <Search size={15} />
            <input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t.social.searchFriends} />
          </div>
          <div className="social-drawer-body">
            <Section label={t.social.requests.toUpperCase()} members={requests} />
            <Section label={t.social.playing.toUpperCase()} members={playing} />
            <Section label={t.social.online.toUpperCase()} members={online} />
            <Section label={t.social.idle.toUpperCase()} members={idle} />
            <Section label={t.social.offline.toUpperCase()} members={offline} defaultOpen={false} />
            {visible.length === 0 ? <div className="social-empty">{t.social.noPeopleMatch}</div> : null}
          </div>
          <footer className="social-drawer-footer">
            <button type="button" onClick={onOpenSocial}><UsersRound size={16} /> {t.social.openSocialHub}</button>
            <button type="button" onClick={() => {
              const self = members.find((member) => member.id === selfId)
              if (self) openProfile(self.id)
            }}><Activity size={16} /> {t.social.myProfile}</button>
          </footer>
        </motion.aside>

        <aside className="social-rail" aria-label={t.social.socialPanel}>
          <button type="button" className={drawerOpen ? 'social-rail-main is-active' : 'social-rail-main'} onClick={() => setDrawerOpen(!drawerOpen)}>
            <UsersRound size={19} />
            <span>{onlineCount}</span>
            {requests.length > 0 ? <i>{requests.length}</i> : null}
          </button>
          <div className="social-rail-avatars">
            {playing.concat(online, idle).slice(0, 6).map((member) => (
              <button type="button" key={member.id} onClick={() => openProfile(member.id)} title={`${member.displayName} · ${member.activity.label}`}>
                <Avatar member={member} size={32} />
              </button>
            ))}
          </div>
          <button type="button" className="social-rail-add" onClick={onOpenSocial} title={t.social.addFriend} aria-label={t.social.addFriend}><UserPlus size={21} strokeWidth={2.1} /></button>
        </aside>
          </motion.div>
        ) : null}
      </AnimatePresence>
      {!coverBannerDismissed && (coverPublishState === 'pendingPublish' || coverPublishState === 'publishFailed') ? (
        <div
          className={`social-cover-publish-status is-${coverPublishState}`}
          role="status"
          aria-live="polite"
          title={coverPublishError || (coverPublishBy ? `Queued for publishing by ${coverPublishBy}` : undefined)}
        >
          <ShieldCheck size={16} />
          <span className="social-cover-publish-copy">
            <span>{coverPublishLabel}</span>
            {coverPublishError ? <small>{coverPublishError}</small> : null}
          </span>
          {coverPublishState === 'publishFailed' ? (
            <button type="button" disabled={coverPublishRetryBusy} onClick={() => {
              setCoverPublishRetryBusy(true)
              void retrySelfCoverPublish().finally(() => setCoverPublishRetryBusy(false))
            }}><RotateCcw size={14} /> {t.social.retryCoverPublish}</button>
          ) : null}
          <button
            type="button"
            className="social-cover-dismiss-btn"
            onClick={() => setCoverBannerDismissed(true)}
            aria-label="Dismiss"
            style={{
              background: 'none',
              border: 'none',
              color: 'rgba(255,255,255,0.6)',
              cursor: 'pointer',
              padding: '2px 4px',
              display: 'flex',
              alignItems: 'center',
              borderRadius: '4px',
            }}
          >
            <X size={14} />
          </button>
        </div>
      ) : null}
      <ProfilePopout />
      <FullProfileModal />
      <ProfileEditorModal />
    </>
  )
}

function RelationshipAction({ member }: { member: SocialMember }) {
  const { t } = useLocale()
  const { selfId, updateRelationship, openFullProfile } = useSocialPrototype()
  if (member.id === selfId) return <button type="button" className="social-secondary" onClick={() => openFullProfile(member.id)}>{t.social.viewProfile}</button>
  if (member.relationship === 'pending-in') {
    return <button type="button" className="social-primary" onClick={() => updateRelationship(member.id, 'friend')}><Check size={15} /> {t.social.accept}</button>
  }
  if (member.relationship === 'pending-out') {
    return <button type="button" className="social-secondary" onClick={() => updateRelationship(member.id, 'none')}>{t.social.cancelRequest}</button>
  }
  if (member.relationship === 'friend') {
    return <button type="button" className="social-secondary" onClick={() => openFullProfile(member.id)}>{t.social.viewProfile}</button>
  }
  return <button type="button" className="social-primary" onClick={() => updateRelationship(member.id, 'pending-out')}><UserPlus size={15} /> {t.social.addFriendAction}</button>
}

function FriendsPane({ mode }: { mode: 'online' | 'all' | 'pending' | 'blocked' }) {
  const { t } = useLocale()
  const { members, selfId, openProfile } = useSocialPrototype()
  const filtered = members.filter((member) => member.id !== selfId).filter((member) => {
    if (mode === 'online') return member.relationship === 'friend' && member.presence !== 'offline'
    if (mode === 'all') return member.relationship === 'friend'
    if (mode === 'pending') return member.relationship === 'pending-in' || member.relationship === 'pending-out'
    return member.relationship === 'blocked'
  })

  return (
    <div className="social-friends-list">
      <div className="social-list-count">{{ online: t.social.online, all: t.social.all, pending: t.social.pending, blocked: t.social.blocked }[mode].toUpperCase()} — {filtered.length}</div>
      {filtered.map((member) => (
        <article key={member.id} className="social-friend-card">
          <button type="button" className="social-friend-main" onClick={() => openProfile(member.id)}>
            <Avatar member={member} size={44} />
            <span><strong>{member.displayName}</strong><small>@{member.username} · {member.activity.label}</small></span>
          </button>
          <div className="social-friend-actions">
            <RelationshipAction member={member} />
            <button type="button" className="social-icon-button" onClick={() => openProfile(member.id)}><MoreHorizontal size={16} /></button>
          </div>
        </article>
      ))}
      {filtered.length === 0 ? <div className="social-hub-empty"><UsersRound size={24} /><strong>{t.social.nothingHere}</strong><span>{t.social.nothingHereDescription}</span></div> : null}
    </div>
  )
}

function AddFriendPane() {
  const { t } = useLocale()
  const { selfId, openProfile, registerMembers } = useSocialPrototype()
  const [query, setQuery] = useState('')
  const deferredQuery = useDeferredValue(query.trim())
  const [matches, setMatches] = useState<SocialMember[]>([])
  const [busy, setBusy] = useState(false)
  const [searchError, setSearchError] = useState<string | null>(null)
  const requestIdRef = useRef(0)

  useEffect(() => {
    if (deferredQuery.length < 2) {
      setMatches([])
      setSearchError(null)
      setBusy(false)
      return
    }
    const requestId = ++requestIdRef.current
    setBusy(true)
    setSearchError(null)
    const timer = window.setTimeout(() => {
      void searchSocialUsers(deferredQuery).then((result) => {
        if (requestId !== requestIdRef.current) return
        const discovered = result.items
          .map(profileDtoToMember)
          .filter((member) => member.id !== selfId)
        registerMembers(discovered)
        setMatches(discovered)
      }).catch((requestError) => {
        if (requestId !== requestIdRef.current) return
        setMatches([])
        setSearchError(requestError instanceof Error ? requestError.message : String(requestError))
      }).finally(() => {
        if (requestId === requestIdRef.current) setBusy(false)
      })
    }, 300)
    return () => window.clearTimeout(timer)
  }, [deferredQuery, registerMembers, selfId])

  return (
    <div className="social-add-friend-pane">
      <div className="social-add-copy">
        <span className="social-card-kicker">{t.social.addFriendKicker}</span>
        <h3>{t.social.findTitle}</h3>
        <p>{t.social.findDescription}</p>
      </div>
      <div className="social-add-search">
        <Search size={18} />
        <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t.social.findPlaceholder} autoFocus />
      </div>
      <div className="social-add-results">
        {matches.map((member) => (
          <article key={member.id}>
            <button type="button" onClick={() => openProfile(member.id)}><Avatar member={member} size={40} /><span><strong>{member.displayName}</strong><small>@{member.username}</small></span></button>
            <RelationshipAction member={member} />
          </article>
        ))}
        {busy ? <div className="social-empty">{t.social.searching}</div> : null}
        {searchError ? <div className="social-empty is-error">{searchError}</div> : null}
        {!busy && !searchError && deferredQuery.length >= 2 && matches.length === 0 ? <div className="social-empty">{t.social.noMatch}</div> : null}
      </div>
    </div>
  )
}

function Leaderboard() {
  const { t } = useLocale()
  const { members, selfId, setLeaderboardParticipation } = useSocialPrototype()
  const [metric, setMetric] = useState<SocialLeaderboardMetric>('downloads')
  const [period, setPeriod] = useState<'all' | 'month' | 'week'>('all')
  const [scope, setScope] = useState<'global' | 'friends'>('global')
  const [entries, setEntries] = useState<Array<{ rank: number; member: SocialMember; score: number }>>([])
  const [busy, setBusy] = useState(true)
  const [participationBusy, setParticipationBusy] = useState(false)
  const [leaderboardError, setLeaderboardError] = useState<string | null>(null)
  const self = members.find((member) => member.id === selfId)
  const optedIn = self?.leaderboardOptIn === true

  useEffect(() => {
    let cancelled = false
    setBusy(true)
    setLeaderboardError(null)
    void getSocialLeaderboard(metric, period, scope).then((result) => {
      if (cancelled) return
      setEntries(result.entries.map((entry) => ({ rank: entry.rank, member: profileDtoToMember(entry.profile), score: entry.score })))
    }).catch((requestError) => {
      if (!cancelled) setLeaderboardError(requestError instanceof Error ? requestError.message : String(requestError))
    }).finally(() => {
      if (!cancelled) setBusy(false)
    })
    return () => { cancelled = true }
  }, [metric, period, scope])

  const selfRank = entries.find((entry) => entry.member.id === selfId)?.rank
  const top = entries.slice(0, 3)
  const rest = entries.slice(3)

  const scoreLabel = (score: number) => {
    if (metric === 'downloads') return `${score} ${t.social.games.toLowerCase()}`
    return formatSocialMinutes(score)
  }

  return (
    <div className="social-leaderboard">
      <div className="social-leaderboard-controls">
        <div className="social-segmented">
          <button type="button" className={metric === 'downloads' ? 'is-active' : ''} onClick={() => setMetric('downloads')}><Download size={15} /> {t.social.downloads}</button>
          <button type="button" className={metric === 'playtime' ? 'is-active' : ''} onClick={() => setMetric('playtime')}><Gamepad2 size={15} /> {t.social.playtime}</button>
          <button type="button" className={metric === 'online' ? 'is-active' : ''} onClick={() => setMetric('online')}><Clock3 size={15} /> {t.social.online}</button>
        </div>
        <div className="social-period-tabs">
          {(['all', 'month', 'week'] as const).map((value) => (
            <button type="button" key={value} className={period === value ? 'is-active' : ''} onClick={() => setPeriod(value)}>
              {value === 'all' ? t.social.allTime : value === 'month' ? t.social.thisMonth : t.social.thisWeek}
            </button>
          ))}
        </div>
        <div className="social-period-tabs">
          <button type="button" className={scope === 'global' ? 'is-active' : ''} onClick={() => setScope('global')}>{t.social.global}</button>
          <button type="button" className={scope === 'friends' ? 'is-active' : ''} onClick={() => setScope('friends')}>{t.social.friends}</button>
        </div>
        <span className="social-my-rank">{t.social.myRank} <strong>{selfRank ? `#${selfRank}` : '—'}</strong></span>
      </div>

      <div className="social-podium">
        {top.map((entry, index) => (
          <article key={entry.member.id} className={`social-podium-card rank-${index + 1}`}>
            <span className="social-rank-medal"><Medal size={20} /><i>#{entry.rank}</i></span>
            <Avatar member={entry.member} size={index === 0 ? 70 : 58} />
            <strong>{entry.member.displayName}</strong>
            <span>{scoreLabel(entry.score)}</span>
            {metric === 'downloads' ? <small>{entry.member.stats.downloadedGb.toLocaleString()} GB {t.social.transferred}</small> : null}
          </article>
        ))}
      </div>

      <div className="social-rank-list">
        {rest.map((entry) => (
          <article key={entry.member.id} className={entry.member.id === selfId ? 'is-self' : ''}>
            <span className="social-rank-number">#{entry.rank}</span>
            <Avatar member={entry.member} size={36} />
            <span className="social-rank-name"><strong>{entry.member.displayName}</strong><small>@{entry.member.username}</small></span>
            <strong className="social-rank-score">{scoreLabel(entry.score)}</strong>
          </article>
        ))}
      </div>
      {busy ? <div className="social-hub-empty"><Activity size={24} /><strong>{t.social.loadingRankings}</strong></div> : null}
      {!busy && !leaderboardError && entries.length === 0 ? <div className="social-hub-empty"><Trophy size={24} /><strong>{t.social.noRankingData}</strong></div> : null}
      {leaderboardError ? <div className="social-prototype-note is-error"><ShieldCheck size={15} /> {leaderboardError}</div> : null}
      <div className="social-leaderboard-participation">
        <div><strong>{optedIn ? t.social.participating : t.social.joinLeaderboard}</strong><span>{t.social.leaderboardNotice}</span></div>
        <button type="button" className={optedIn ? 'social-secondary' : 'social-primary'} disabled={participationBusy} onClick={() => {
          setParticipationBusy(true)
          void setLeaderboardParticipation(!optedIn).finally(() => setParticipationBusy(false))
        }}>{optedIn ? t.social.leaveLeaderboard : t.social.participate}</button>
      </div>
    </div>
  )
}

export function SocialHubView() {
  const { t } = useLocale()
  const {
    members,
    selfId,
    openFullProfile,
    openProfileEditor,
    coverUrl,
    localProfile,
    loading,
    error,
    migrationPending,
    migrateLocalProfile,
    dismissLocalProfileMigration,
  } = useSocialPrototype()
  const self = members.find((member) => member.id === selfId) ?? members[0]
  const pendingCount = members.filter((member) => member.relationship === 'pending-in').length
  const [tab, setTab] = useState<'friends' | 'pending' | 'leaderboards' | 'profile'>('friends')
  const [friendMode, setFriendMode] = useState<'online' | 'all' | 'pending' | 'blocked'>('online')
  const [showAddFriend, setShowAddFriend] = useState(false)
  const serviceNotice = error && /SOCIAL_REQUEST_FAILED|HTTP 404/i.test(error)
    ? t.social.serviceUnavailableLocal
    : error

  return (
    <section className="social-hub-view">
      <header className="social-hub-header">
        <div>
          <span className="social-hub-eyebrow"><UsersRound size={15} /> {t.social.eyebrow} {loading ? <i>{t.social.connecting}</i> : null}</span>
          <h1>{t.social.title}</h1>
          <p>{t.social.subtitle}</p>
        </div>
        <button type="button" className="social-primary social-add-friend-button" onClick={() => setShowAddFriend((value) => !value)}><UserPlus size={16} /> {t.social.addFriend}</button>
      </header>
      {serviceNotice ? <div className="social-hub-service-notice"><ShieldCheck size={15} /><span>{serviceNotice}</span></div> : null}
      {migrationPending ? (
        <div className="social-profile-migration-notice">
          <div><ShieldCheck size={18} /><span><strong>{t.social.migrationTitle}</strong>{t.social.migrationDescription}</span></div>
          <button type="button" className="social-secondary" onClick={dismissLocalProfileMigration}>{t.social.keepLocal}</button>
          <button type="button" className="social-primary" onClick={() => void migrateLocalProfile()}>{t.social.migrate}</button>
        </div>
      ) : null}

      <nav className="social-hub-tabs">
        <button type="button" className={tab === 'friends' ? 'is-active' : ''} onClick={() => setTab('friends')}><UsersRound size={16} /> {t.social.friends}</button>
        <button type="button" className={tab === 'pending' ? 'is-active' : ''} onClick={() => setTab('pending')}><BellDot size={16} /> {t.social.requests} {pendingCount > 0 ? <i>{pendingCount}</i> : null}</button>
        <button type="button" className={tab === 'leaderboards' ? 'is-active' : ''} onClick={() => setTab('leaderboards')}><Trophy size={16} /> {t.social.leaderboards}</button>
        <button type="button" className={tab === 'profile' ? 'is-active' : ''} onClick={() => setTab('profile')}><Activity size={16} /> {t.social.myProfile}</button>
      </nav>

      <AnimatePresence initial={false}>
        {showAddFriend ? (
          <motion.div className="social-add-friend-wrap" initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: 'auto' }} exit={{ opacity: 0, height: 0 }}>
            <AddFriendPane />
          </motion.div>
        ) : null}
      </AnimatePresence>

      <div className="social-hub-content">
        {tab === 'friends' ? (
          <>
            <div className="social-friend-filter-bar">
              <div className="social-segmented">
                <button type="button" className={friendMode === 'online' ? 'is-active' : ''} onClick={() => setFriendMode('online')}>{t.social.online}</button>
                <button type="button" className={friendMode === 'all' ? 'is-active' : ''} onClick={() => setFriendMode('all')}>{t.social.all}</button>
                <button type="button" className={friendMode === 'pending' ? 'is-active' : ''} onClick={() => setFriendMode('pending')}>{t.social.pending}</button>
                <button type="button" className={friendMode === 'blocked' ? 'is-active' : ''} onClick={() => setFriendMode('blocked')}>{t.social.blocked}</button>
              </div>
            </div>
            <FriendsPane mode={friendMode} />
          </>
        ) : tab === 'pending' ? <FriendsPane mode="pending" /> : tab === 'leaderboards' ? <Leaderboard /> : self ? (
          <div className="social-my-profile-card">
            <button
              type="button"
              className={`social-my-profile-banner social-my-profile-banner-button${coverUrl ? ' has-cover' : ''}`}
              style={profileBannerStyle(self, selfId, coverUrl, localProfile)}
              onClick={openProfileEditor}
              title={t.social.customize}
            >
              <span><Pencil size={14} /> {coverUrl ? t.social.changeCover : t.social.addCover}</span>
            </button>
            <div className="social-my-profile-body">
              <Avatar member={self} size={96} />
              <div className="social-my-profile-copy">
                <span className="social-card-kicker">{t.social.profileKicker}</span>
                <h2>{self.displayName}</h2>
                <p>@{self.username}</p>
                <div className="social-badge-row">{self.badges.map((badge) => <span key={badge}><Medal size={13} />{badge}</span>)}</div>
              </div>
              <div className="social-my-profile-actions">
                <button type="button" className="social-secondary" onClick={openProfileEditor}><Pencil size={14} /> {t.social.customize}</button>
                <button type="button" className="social-primary" onClick={() => openFullProfile(self.id)}>{t.social.openFullProfile}</button>
              </div>
            </div>
            <div className="social-my-profile-stats">
              <article><Download size={18} /><strong>{self.stats.downloads}</strong><span>{t.social.downloads}</span></article>
              <article><Gamepad2 size={18} /><strong>{formatSocialMinutes(self.stats.playMinutes)}</strong><span>{t.social.playtime}</span></article>
              <article><Clock3 size={18} /><strong>{formatSocialMinutes(self.stats.onlineMinutes)}</strong><span>{t.social.online}</span></article>
              <article><BarChart3 size={18} /><strong>{self.stats.gamesPlayed}</strong><span>{t.social.games}</span></article>
            </div>
          </div>
        ) : null}
      </div>
    </section>
  )
}
