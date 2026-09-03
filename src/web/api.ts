import type {
  LauncherDevice,
  LegalAcceptance,
  RemoteJob,
  RemoteJobAction,
  WebCatalog,
  WebSession,
} from './types'

const TENANT = import.meta.env.VITE_BACKEND_TENANT || '0xolemon'
const API_ROOT = `/api/${encodeURIComponent(TENANT)}`

export class WebApiError extends Error {
  status: number
  code: string

  constructor(status: number, code: string) {
    super(code)
    this.name = 'WebApiError'
    this.status = status
    this.code = code
  }
}

function cookieValue(name: string): string {
  const prefix = `${encodeURIComponent(name)}=`
  const entry = document.cookie.split(';').map((item) => item.trim()).find((item) => item.startsWith(prefix))
  return entry ? decodeURIComponent(entry.slice(prefix.length)) : ''
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const method = String(init.method || 'GET').toUpperCase()
  const headers = new Headers(init.headers)
  if (init.body && !headers.has('content-type')) headers.set('content-type', 'application/json')
  if (!['GET', 'HEAD', 'OPTIONS'].includes(method)) {
    const csrf = cookieValue('0xolemon_csrf')
    if (csrf) headers.set('x-csrf-token', csrf)
  }

  const response = await fetch(`${API_ROOT}${path}`, {
    ...init,
    headers,
    credentials: 'include',
    cache: 'no-store',
  })

  if (!response.ok) {
    const body = await response.json().catch(() => ({})) as { error?: string }
    throw new WebApiError(response.status, body.error || `HTTP_${response.status}`)
  }
  if (response.status === 204) return undefined as T
  const contentType = response.headers.get('content-type') || ''
  if (!contentType.toLocaleLowerCase().includes('application/json')) {
    throw new WebApiError(502, 'BACKEND_UNAVAILABLE')
  }
  try {
    return await response.json() as T
  } catch {
    throw new WebApiError(502, 'BACKEND_INVALID_RESPONSE')
  }
}

export const webApi = {
  apiRoot: API_ROOT,
  session: () => request<WebSession>('/auth/session'),
  loginUrl: (returnTo = '/app') => `${API_ROOT}/auth/discord/start?returnTo=${encodeURIComponent(returnTo)}`,
  logout: () => request<void>('/auth/logout', { method: 'POST' }),
  acceptLegal: (value: Pick<LegalAcceptance, 'termsVersion' | 'privacyVersion'> & { locale: 'en' | 'vi' }) =>
    request<LegalAcceptance>('/legal/acceptance', { method: 'POST', body: JSON.stringify(value) }),
  catalog: () => request<WebCatalog>('/web/catalog'),
  devices: async () => (await request<{ devices: LauncherDevice[] }>('/devices')).devices,
  revokeDevice: (deviceId: string) => request<void>(`/devices/${encodeURIComponent(deviceId)}`, { method: 'DELETE' }),
  jobs: async () => (await request<{ jobs: RemoteJob[] }>('/remote-jobs')).jobs,
  createJob: (value: {
    action: RemoteJobAction
    gameId: string
    versionId: string
    deviceId: string
    libraryId: string
    requestId: string
  }) => request<RemoteJob>('/remote-jobs', { method: 'POST', body: JSON.stringify(value) }),
  cancelJob: (jobId: string) => request<RemoteJob>(`/remote-jobs/${encodeURIComponent(jobId)}/cancel`, { method: 'POST' }),
}

export function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1)
  return `${(value / 1024 ** index).toFixed(index > 2 ? 1 : 0)} ${units[index]}`
}

export function createRequestId(): string {
  return crypto.randomUUID()
}
