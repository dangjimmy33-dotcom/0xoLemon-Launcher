import { useCallback, useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  Circle,
  Clock3,
  Download,
  FileCheck2,
  KeyRound,
  Loader2,
  LogIn,
  PackageCheck,
  RefreshCw,
  ShieldCheck,
  Users,
  X,
} from 'lucide-react'
import { useLocale } from '../context/locale'
import { formatBytes } from '../lib/format'

const SUPPORTED_GAME_ID = 'ea-sports-fc-26'
const ACTIVATION_EVENT = 'launcher://offline-activation'
const ACTIVATION_SSE_URL =
  'https://zeroxolemon-launcher.onrender.com/api/0xolemon/offline-activation/ea-sports-fc-26/events'

type ActivationStep = {
  id: string
  status: 'pending' | 'running' | 'completed' | 'failed'
  progress: number
}

type OfflineActivationState = {
  gameId: string
  status: 'idle' | 'running' | 'paused' | 'completed' | 'failed' | 'canceled'
  phase: string
  progress: number
  bytesDownloaded: number
  totalBytes: number
  cancellable: boolean
  canResume: boolean
  messageCode?: string | null
  errorCode?: string | null
  requestId?: string | null
  capacity: number
  available: number
  inUse: number
  reservations: number
  nextAvailableAt?: string | null
  serverTime?: string | null
  accountEligible?: boolean | null
  nextEligibleAt?: string | null
  backendReady: boolean
  backendMissingConfiguration: string[]
  packageVersion?: string | null
  packageSha256?: string | null
  steps: ActivationStep[]
}

const EMPTY_STATE: OfflineActivationState = {
  gameId: SUPPORTED_GAME_ID,
  status: 'idle',
  phase: 'idle',
  progress: 0,
  bytesDownloaded: 0,
  totalBytes: 0,
  cancellable: false,
  canResume: false,
  capacity: 5,
  available: 0,
  inUse: 0,
  reservations: 0,
  backendReady: false,
  backendMissingConfiguration: [],
  steps: [],
}

const STEP_ICONS = {
  validate: ShieldCheck,
  package: PackageCheck,
  ticket: FileCheck2,
  request: Users,
  apply: KeyRound,
  launch: LogIn,
}

const ERROR_KEYS: Record<string, string> = {
  ACCOUNT_COOLDOWN: 'accountCooldown',
  ACCOUNT_RATE_LIMITED: 'accountRateLimited',
  ACCOUNT_REQUEST_ACTIVE: 'accountRequestActive',
  ACCOUNT_TOO_NEW: 'accountTooNew',
  AUTH_FORBIDDEN: 'authForbidden',
  AUTH_REQUIRED: 'authRequired',
  AUTH_UNAVAILABLE: 'authUnavailable',
  BACKEND_RESPONSE_INVALID: 'backendResponseInvalid',
  BACKEND_UNAVAILABLE: 'backendUnavailable',
  CANCELED: 'canceled',
  CONFIG_TOKEN_FIELD_MISSING: 'configInvalid',
  CONFIG_VERIFY_FAILED: 'configInvalid',
  EA_REJECTED: 'eaRejected',
  GAME_FILES_INVALID: 'gameFilesInvalid',
  GAME_FILES_MISSING: 'gameFilesInvalid',
  GAME_NOT_INSTALLED: 'gameNotInstalled',
  GAME_PATH_CHANGED: 'gamePathChanged',
  GAME_PATH_INVALID: 'gamePathInvalid',
  IDEMPOTENCY_CONFLICT: 'idempotencyConflict',
  INVALID_PATH: 'gamePathInvalid',
  LAUNCHER_UPDATE_REQUIRED: 'launcherUpdateRequired',
  NETWORK_CLIENT_FAILED: 'backendUnavailable',
  NO_GLOBAL_SLOT: 'noGlobalSlot',
  OUTCOME_UNCERTAIN: 'outcomeUncertain',
  PACKAGE_DOWNLOAD_FAILED: 'packageDownloadFailed',
  PACKAGE_EXTRACT_FAILED: 'packageExtractFailed',
  PACKAGE_INTEGRITY_FAILED: 'packageIntegrityFailed',
  PACKAGE_METADATA_INVALID: 'packageMetadataInvalid',
  PACKAGE_PATH_INVALID: 'packageIntegrityFailed',
  REQUEST_IN_PROGRESS: 'requestInProgress',
  REQUEST_STATE_INVALID: 'requestStateInvalid',
  SERVICE_UNAVAILABLE: 'serviceUnavailable',
  TICKET_INVALID: 'ticketInvalid',
  TICKET_TIMEOUT: 'ticketTimeout',
  TOKEN_INVALID: 'tokenInvalid',
}

function clampProgress(value: number) {
  return Math.max(0, Math.min(1, Number.isFinite(value) ? value : 0))
}

function formatRemaining(target: string | null | undefined, serverNowMs: number, ready: string) {
  if (!target) return ready
  const targetMs = Date.parse(target)
  if (!Number.isFinite(targetMs)) return ready
  const remaining = Math.max(0, targetMs - serverNowMs)
  if (remaining <= 0) return ready
  const totalSeconds = Math.ceil(remaining / 1000)
  const days = Math.floor(totalSeconds / 86400)
  const hours = Math.floor((totalSeconds % 86400) / 3600)
  const minutes = Math.floor((totalSeconds % 3600) / 60)
  const seconds = totalSeconds % 60
  if (days > 0) return `${days}d ${hours}h ${minutes}m`
  if (hours > 0) return `${hours}h ${minutes}m ${seconds}s`
  return `${minutes}m ${seconds}s`
}

export function DenuvoActivationButton({ gameId }: { gameId: string }) {
  const { t } = useLocale()
  const copy = t.offlineActivation.activation
  const [state, setState] = useState<OfflineActivationState>(EMPTY_STATE)
  const [loadingState, setLoadingState] = useState(true)
  const [actionPending, setActionPending] = useState(false)
  const [actionFailed, setActionFailed] = useState(false)
  const [serverOffsetMs, setServerOffsetMs] = useState(0)
  const [clockMs, setClockMs] = useState(() => Date.now())

  const acceptState = useCallback((next: OfflineActivationState) => {
    if (next.serverTime) {
      const serverTime = Date.parse(next.serverTime)
      if (Number.isFinite(serverTime)) setServerOffsetMs(serverTime - Date.now())
    }
    setState(next)
    setActionFailed(false)
  }, [])

  const refreshState = useCallback(async (quiet = false) => {
    if (!quiet) setLoadingState(true)
    try {
      const next = await invoke<OfflineActivationState>('get_offline_activation_state', { gameId })
      acceptState(next)
    } catch {
      if (!quiet) setActionFailed(true)
    } finally {
      if (!quiet) setLoadingState(false)
    }
  }, [acceptState, gameId])

  useEffect(() => {
    if (gameId !== SUPPORTED_GAME_ID) return
    let disposed = false
    let unlisten: (() => void) | undefined

    void listen<OfflineActivationState>(ACTIVATION_EVENT, (event) => {
      if (!disposed) acceptState(event.payload)
    }).then((dispose) => {
      if (disposed) dispose()
      else unlisten = dispose
    })
    const initialRefresh = window.setTimeout(() => void refreshState(), 0)

    return () => {
      disposed = true
      window.clearTimeout(initialRefresh)
      unlisten?.()
    }
  }, [acceptState, gameId, refreshState])

  useEffect(() => {
    if (gameId !== SUPPORTED_GAME_ID || typeof EventSource === 'undefined') return
    const source = new EventSource(ACTIVATION_SSE_URL)
    const onQuota = () => void refreshState(true)
    source.addEventListener('quota', onQuota)
    return () => {
      source.removeEventListener('quota', onQuota)
      source.close()
    }
  }, [gameId, refreshState])

  useEffect(() => {
    const timer = window.setInterval(() => setClockMs(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [])

  const run = useCallback(async (
    command: 'start_offline_activation' | 'resume_offline_activation' | 'cancel_offline_activation',
  ) => {
    setActionPending(true)
    setActionFailed(false)
    try {
      const args = command === 'start_offline_activation' ? { gameId } : undefined
      const next = await invoke<OfflineActivationState>(command, args)
      acceptState(next)
    } catch {
      await refreshState(true)
      setActionFailed(true)
    } finally {
      setActionPending(false)
    }
  }, [acceptState, gameId, refreshState])

  if (gameId !== SUPPORTED_GAME_ID) return null

  const isRunning = state.status === 'running'
  const canStart =
    !loadingState &&
    !actionPending &&
    !isRunning &&
    !state.canResume &&
    state.backendReady &&
    state.available > 0 &&
    state.accountEligible === true
  const serverNowMs = clockMs + serverOffsetMs
  const slotCountdown = formatRemaining(state.nextAvailableAt, serverNowMs, copy.availableNow)
  const accountCountdown = formatRemaining(state.nextEligibleAt, serverNowMs, copy.eligibleNow)
  const messageName = state.messageCode?.startsWith('activation.')
    ? state.messageCode.slice('activation.'.length)
    : ''
  const messages = copy.messages as Record<string, string>
  const errors = copy.errors as Record<string, string>
  const errorName = state.errorCode ? ERROR_KEYS[state.errorCode] : ''
  const statusMessage = state.status === 'failed' || state.status === 'canceled'
    ? errors[errorName] || errors.generic
    : messages[messageName] || messages[state.phase] || messages.idle
  const packageHash = state.packageSha256 ? state.packageSha256.slice(0, 12) : copy.notAvailable

  return (
    <section className="denuvo-activation" aria-live="polite">
      <div className="activation-summary">
        <div className="activation-quota-block">
          <span className="activation-summary-icon"><Users size={18} /></span>
          <div>
            <small>{copy.globalCapacity}</small>
            <strong>{copy.remaining.replace('{available}', String(state.available)).replace('{capacity}', String(state.capacity))}</strong>
            <span>
              {copy.inUse
                .replace('{count}', String(state.inUse))
                .replace('{reservations}', String(state.reservations))}
            </span>
          </div>
        </div>
        <div className={`activation-service-state${state.backendReady ? ' is-ready' : ' is-unavailable'}`}>
          {state.backendReady ? <CheckCircle2 size={15} /> : <AlertTriangle size={15} />}
          <span>{state.backendReady ? copy.backendReady : copy.backendUnavailable}</span>
        </div>
      </div>

      <div className="activation-facts">
        <div>
          <Clock3 size={15} />
          <span>{copy.nextGlobalSlot}</span>
          <strong>{state.available > 0 ? copy.availableNow : slotCountdown}</strong>
        </div>
        <div>
          <ShieldCheck size={15} />
          <span>{copy.accountStatus}</span>
          <strong>
            {state.accountEligible === true
              ? copy.eligibleNow
              : state.nextEligibleAt
                ? copy.eligibleIn.replace('{time}', accountCountdown)
                : copy.signInRequired}
          </strong>
        </div>
        <div>
          <PackageCheck size={15} />
          <span>{copy.packageLabel}</span>
          <strong>{state.packageVersion || copy.notAvailable} · {packageHash}</strong>
        </div>
      </div>

      {(isRunning || state.status === 'paused' || state.status === 'completed' || state.status === 'failed' || state.status === 'canceled') && (
        <div className={`activation-current-state is-${state.status}`}>
          <div className="activation-current-heading">
            <span>
              {isRunning && <Loader2 size={16} className="spin" />}
              {state.status === 'paused' && <Clock3 size={16} />}
              {state.status === 'completed' && <CheckCircle2 size={16} />}
              {(state.status === 'failed' || state.status === 'canceled') && <AlertTriangle size={16} />}
              {statusMessage}
            </span>
            <strong>{Math.round(clampProgress(state.progress) * 100)}%</strong>
          </div>
          <progress className="activation-overall-progress" max={1} value={clampProgress(state.progress)} />
          {state.totalBytes > 0 && (
            <small>{formatBytes(state.bytesDownloaded)} / {formatBytes(state.totalBytes)}</small>
          )}
        </div>
      )}

      <ol className="activation-step-list">
        {state.steps.map((step) => {
          const Icon = STEP_ICONS[step.id as keyof typeof STEP_ICONS] || Circle
          const stepLabels = copy.steps as Record<string, string>
          return (
            <li key={step.id} className={`is-${step.status}`}>
              <span className="activation-step-icon">
                {step.status === 'completed' ? <Check size={14} /> : <Icon size={14} />}
              </span>
              <span>{stepLabels[step.id] || step.id}</span>
              {step.status === 'running' ? <Loader2 size={13} className="spin" /> : null}
            </li>
          )
        })}
      </ol>

      {actionFailed && (
        <div className="activation-action-error" role="alert">
          <AlertTriangle size={14} /> {copy.actionFailed}
        </div>
      )}

      <div className="activation-actions">
        <button
          type="button"
          className="activation-primary-action"
          onClick={() => void run('start_offline_activation')}
          disabled={!canStart}
        >
          {actionPending || isRunning ? <Loader2 size={15} className="spin" /> : <KeyRound size={15} />}
          {isRunning ? copy.activating : copy.activate}
        </button>
        {state.canResume && !isRunning && (
          <button
            type="button"
            className="activation-secondary-action"
            onClick={() => void run('resume_offline_activation')}
            disabled={actionPending}
          >
            <Download size={15} /> {copy.resume}
          </button>
        )}
        {isRunning && state.cancellable && (
          <button
            type="button"
            className="activation-secondary-action"
            onClick={() => void run('cancel_offline_activation')}
            disabled={actionPending}
          >
            <X size={15} /> {copy.cancel}
          </button>
        )}
        <button
          type="button"
          className="activation-icon-action"
          title={copy.refresh}
          aria-label={copy.refresh}
          onClick={() => void refreshState()}
          disabled={loadingState || actionPending}
        >
          <RefreshCw size={15} className={loadingState ? 'spin' : ''} />
        </button>
      </div>

      {!state.backendReady && state.backendMissingConfiguration.length > 0 && (
        <p className="activation-service-note">{copy.serviceUnavailable}</p>
      )}
    </section>
  )
}
