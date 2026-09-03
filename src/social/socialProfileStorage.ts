import { open } from '@tauri-apps/plugin-dialog'
import {
  BaseDirectory,
  exists,
  mkdir,
  readFile,
  readTextFile,
  remove,
  writeFile,
  writeTextFile,
} from '@tauri-apps/plugin-fs'
import { isTauriRuntime } from '../lib/tauriRuntime'

const PROFILE_ROOT = 'social-profile'
const PROFILE_FILE_NAME = 'profile.json'
const COVER_FILE_NAME = 'cover.bin'
const PREVIOUS_PROFILE_NAME = 'backups/profile.previous.json'
const PREVIOUS_COVER_NAME = 'backups/cover.previous.bin'
const FALLBACK_STORAGE_KEY = '0xo_social_local_profile_v1'
const MAX_COVER_BYTES = 12 * 1024 * 1024

export type LocalSocialProfileDocument = {
  version: 1
  discordId: string
  bio: string
  customStatus: string
  coverPosition: number
  coverMime?: string
  updatedAt: string
}

export type LocalSocialProfileSnapshot = {
  profile: LocalSocialProfileDocument
  coverBytes?: Uint8Array
}

export type LocalCoverSelection = {
  bytes: Uint8Array
  mime: string
  sourceName: string
}

export type SaveLocalSocialProfileOptions = {
  cover?: LocalCoverSelection | null
}

type ProfilePaths = {
  root: string
  backupDir: string
  profile: string
  cover: string
  previousProfile: string
  previousCover: string
}

function safeIdentitySegment(discordId: string) {
  const safe = discordId.replace(/[^a-zA-Z0-9_-]/g, '').slice(0, 80)
  return safe || 'prototype-self'
}

function pathsFor(discordId: string): ProfilePaths {
  const root = `${PROFILE_ROOT}/${safeIdentitySegment(discordId)}`
  return {
    root,
    backupDir: `${root}/backups`,
    profile: `${root}/${PROFILE_FILE_NAME}`,
    cover: `${root}/${COVER_FILE_NAME}`,
    previousProfile: `${root}/${PREVIOUS_PROFILE_NAME}`,
    previousCover: `${root}/${PREVIOUS_COVER_NAME}`,
  }
}

export function createDefaultLocalSocialProfile(discordId: string, bio: string): LocalSocialProfileDocument {
  return {
    version: 1,
    discordId,
    bio,
    customStatus: '',
    coverPosition: 50,
    updatedAt: new Date().toISOString(),
  }
}

function clampCoverPosition(value: number) {
  if (!Number.isFinite(value)) return 50
  return Math.max(0, Math.min(100, Math.round(value)))
}

function normalizeProfile(raw: Partial<LocalSocialProfileDocument> | null | undefined, fallback: LocalSocialProfileDocument) {
  if (!raw || (raw.discordId && raw.discordId !== fallback.discordId)) return fallback
  return {
    version: 1 as const,
    discordId: fallback.discordId,
    bio: typeof raw.bio === 'string' ? raw.bio.slice(0, 480) : fallback.bio,
    customStatus: typeof raw.customStatus === 'string' ? raw.customStatus.slice(0, 120) : '',
    coverPosition: clampCoverPosition(Number(raw.coverPosition)),
    coverMime: typeof raw.coverMime === 'string' && raw.coverMime ? raw.coverMime : undefined,
    updatedAt: typeof raw.updatedAt === 'string' && raw.updatedAt ? raw.updatedAt : fallback.updatedAt,
  }
}

function fallbackKey(discordId: string) {
  return `${FALLBACK_STORAGE_KEY}:${safeIdentitySegment(discordId)}`
}

function readFallback(fallback: LocalSocialProfileDocument) {
  try {
    const raw = window.localStorage.getItem(fallbackKey(fallback.discordId))
    return raw ? normalizeProfile(JSON.parse(raw) as Partial<LocalSocialProfileDocument>, fallback) : fallback
  } catch {
    return fallback
  }
}

function writeFallback(profile: LocalSocialProfileDocument) {
  try {
    window.localStorage.setItem(fallbackKey(profile.discordId), JSON.stringify(profile))
  } catch {
    // Browser fallback is best-effort only.
  }
}

async function ensureProfileFolders(discordId: string) {
  const paths = pathsFor(discordId)
  await mkdir(paths.backupDir, { baseDir: BaseDirectory.AppLocalData, recursive: true })
  return paths
}

async function readProfileFile(path: string, fallback: LocalSocialProfileDocument) {
  try {
    if (!await exists(path, { baseDir: BaseDirectory.AppLocalData })) return null
    const text = await readTextFile(path, { baseDir: BaseDirectory.AppLocalData })
    return normalizeProfile(JSON.parse(text) as Partial<LocalSocialProfileDocument>, fallback)
  } catch {
    return null
  }
}

async function readCoverFile(path: string) {
  try {
    if (!await exists(path, { baseDir: BaseDirectory.AppLocalData })) return undefined
    return await readFile(path, { baseDir: BaseDirectory.AppLocalData })
  } catch {
    return undefined
  }
}

export async function loadLocalSocialProfile(fallback: LocalSocialProfileDocument): Promise<LocalSocialProfileSnapshot> {
  if (!isTauriRuntime()) return { profile: readFallback(fallback) }

  try {
    const paths = await ensureProfileFolders(fallback.discordId)
    const profile = await readProfileFile(paths.profile, fallback) ?? readFallback(fallback)
    const coverBytes = profile.coverMime ? await readCoverFile(paths.cover) : undefined
    writeFallback(profile)
    return { profile, coverBytes }
  } catch {
    return { profile: readFallback(fallback) }
  }
}

async function snapshotCurrentFiles(fallback: LocalSocialProfileDocument, paths: ProfilePaths) {
  const currentProfile = await readProfileFile(paths.profile, fallback)
  if (currentProfile) {
    await writeTextFile(paths.previousProfile, JSON.stringify(currentProfile, null, 2), { baseDir: BaseDirectory.AppLocalData })
  }
  const currentCover = await readCoverFile(paths.cover)
  if (currentCover?.byteLength) {
    await writeFile(paths.previousCover, currentCover, { baseDir: BaseDirectory.AppLocalData })
  } else if (await exists(paths.previousCover, { baseDir: BaseDirectory.AppLocalData })) {
    await remove(paths.previousCover, { baseDir: BaseDirectory.AppLocalData })
  }
}

export async function saveLocalSocialProfile(
  input: LocalSocialProfileDocument,
  fallback: LocalSocialProfileDocument,
  options: SaveLocalSocialProfileOptions = {},
): Promise<LocalSocialProfileSnapshot> {
  const profile: LocalSocialProfileDocument = normalizeProfile({
    ...input,
    version: 1,
    discordId: fallback.discordId,
    updatedAt: new Date().toISOString(),
    coverMime: options.cover === null ? undefined : options.cover?.mime ?? input.coverMime,
  }, fallback)

  if (!isTauriRuntime()) {
    writeFallback(profile)
    return { profile }
  }

  const paths = await ensureProfileFolders(fallback.discordId)
  await snapshotCurrentFiles(fallback, paths)
  await writeTextFile(paths.profile, JSON.stringify(profile, null, 2), { baseDir: BaseDirectory.AppLocalData })

  if (options.cover === null) {
    if (await exists(paths.cover, { baseDir: BaseDirectory.AppLocalData })) {
      await remove(paths.cover, { baseDir: BaseDirectory.AppLocalData })
    }
  } else if (options.cover) {
    await writeFile(paths.cover, options.cover.bytes, { baseDir: BaseDirectory.AppLocalData })
  }

  writeFallback(profile)
  return {
    profile,
    coverBytes: profile.coverMime ? await readCoverFile(paths.cover) : undefined,
  }
}

export async function restorePreviousLocalSocialProfile(
  fallback: LocalSocialProfileDocument,
): Promise<LocalSocialProfileSnapshot | null> {
  if (!isTauriRuntime()) return null
  const paths = await ensureProfileFolders(fallback.discordId)
  const previous = await readProfileFile(paths.previousProfile, fallback)
  if (!previous) return null
  const previousCover = previous.coverMime ? await readCoverFile(paths.previousCover) : undefined
  const cover = previous.coverMime && previousCover?.byteLength
    ? { bytes: previousCover, mime: previous.coverMime, sourceName: 'backup' }
    : null
  return saveLocalSocialProfile(previous, fallback, { cover })
}

function mimeFromPath(path: string) {
  const lower = path.toLowerCase()
  if (lower.endsWith('.png')) return 'image/png'
  if (lower.endsWith('.webp')) return 'image/webp'
  if (lower.endsWith('.jpg') || lower.endsWith('.jpeg')) return 'image/jpeg'
  return 'image/jpeg'
}

export async function chooseLocalProfileCover(): Promise<LocalCoverSelection | null> {
  if (!isTauriRuntime()) return null
  const selected = await open({
    multiple: false,
    title: 'Choose profile cover',
    filters: [{ name: 'Profile cover', extensions: ['png', 'jpg', 'jpeg', 'webp'] }],
  })
  if (!selected || Array.isArray(selected)) return null

  const bytes = await readFile(selected)
  if (!bytes.byteLength) throw new Error('The selected image is empty.')
  if (bytes.byteLength > MAX_COVER_BYTES) throw new Error('Profile covers must be 12 MB or smaller.')

  const sourceName = selected.replace(/\\/g, '/').split('/').pop() || 'cover'
  return { bytes, mime: mimeFromPath(selected), sourceName }
}
