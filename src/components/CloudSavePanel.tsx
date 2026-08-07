import { useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import {
  AlertTriangle,
  Archive,
  CheckCircle2,
  ChevronRight,
  Cloud,
  CloudDownload,
  CloudOff,
  CloudUpload,
  FolderPlus,
  HardDrive,
  History,
  LogIn,
  LogOut,
  Pin,
  PinOff,
  RefreshCcw,
  RotateCcw,
  ShieldCheck,
  Sparkles,
} from 'lucide-react'
import type { CloudSaveStatus } from '../types'
import { formatBytes } from '../lib/format'
import { cloudSavePresentation, quotaPercent } from '../lib/cloudSaveStatus'
import { useLocale } from '../context/LocaleContext'

function formatDate(value: string | null | undefined, locale: string, empty: string) {
  if (!value) return empty
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString(locale)
}

export function CloudSavePanel({
  status,
  busy,
  launchBlocked,
  onToggle,
  onAddFolder,
  onSync,
  onResolve,
  onRestore,
  onLaunchWithoutSync,
  onConnectGoogleDrive,
  onDisconnectGoogleDrive,
  onBackupGoogleDrive,
  onRestoreMissingFiles,
}: {
  status: CloudSaveStatus | null
  busy: boolean
  launchBlocked: boolean
  onToggle: (enabled: boolean) => void
  onAddFolder: () => void
  onSync: () => void
  onResolve: (conflictId: string, resolution: 'local' | 'cloud') => void
  onRestore: (snapshotId: string) => void
  onLaunchWithoutSync: () => void
  onConnectGoogleDrive: () => void
  onDisconnectGoogleDrive: () => void
  onBackupGoogleDrive: () => void
  onRestoreMissingFiles: () => void
}) {
  const { t, locale } = useLocale()
  const c = t.cloudSave
  const enabled = status?.enabled ?? false
  const roots = status?.saveRoots ?? []
  const conflicts = status?.conflicts ?? []
  const snapshots = status?.snapshots ?? []
  const presentation = cloudSavePresentation(status, c.states)
  const quota = quotaPercent(status?.quota ?? null)
  const [localBusy, setLocalBusy] = useState<string | null>(null)
  const [advancedOpen, setAdvancedOpen] = useState(false)

  const latestSnapshot = useMemo(() => snapshots[0] ?? null, [snapshots])
  const isBusy = busy || localBusy !== null

  async function togglePin(snapshotId: string, pinned: boolean) {
    if (!status) return
    setLocalBusy(`pin:${snapshotId}`)
    try {
      await invoke('pin_cloud_save_snapshot', {
        gameId: status.gameId,
        snapshotId,
        pinned,
      })
      onSync()
    } finally {
      setLocalBusy(null)
    }
  }

  async function exportSnapshot(snapshotId?: string) {
    if (!status) return
    const target = await open({
      directory: true,
      multiple: false,
      title: c.chooseExportFolder,
    })
    if (typeof target !== 'string') return
    setLocalBusy(`export:${snapshotId ?? 'cloud'}`)
    try {
      await invoke('export_cloud_save_snapshot', {
        gameId: status.gameId,
        snapshotId: snapshotId ?? null,
        target,
      })
    } finally {
      setLocalBusy(null)
    }
  }

  return (
    <section className={`panel cloud-save-panel cloud-protection-status tone-${presentation.tone}${conflicts.length ? ' has-conflict' : ''}`}>
      <header className="cloud-save-panel-header">
        <div className="cloud-save-heading">
          <span className="cloud-save-heading-icon"><ShieldCheck size={19} /></span>
          <div>
            <strong>{c.automaticProtection}</strong>
            <small>{c.privateDriveArea}</small>
          </div>
        </div>
        <button
          className={enabled ? 'cloud-save-toggle is-on' : 'cloud-save-toggle'}
          type="button"
          role="switch"
          aria-checked={enabled}
          aria-label={c.toggleAria}
          disabled={isBusy || !status?.mapStatus?.healthy}
          onClick={() => onToggle(!enabled)}
        >
          <span />
        </button>
      </header>

      <div className="cloud-save-primary-status" role="status" aria-live="polite">
        <span className="cloud-save-status-icon">
          {presentation.tone === 'success' ? <CheckCircle2 size={21} /> : presentation.tone === 'danger' ? <AlertTriangle size={21} /> : status?.state === 'offline' ? <CloudOff size={21} /> : <Cloud size={21} />}
        </span>
        <div>
          <strong>{presentation.title}</strong>
          <p>{presentation.description}</p>
        </div>
        {status?.pendingOperationCount ? (
          <span className="cloud-save-pending-chip">
            {c.pendingLabel} · {formatBytes(status.pendingUploadBytes)}
          </span>
        ) : null}
      </div>

      <div className="cloud-save-facts">
        <div>
          <HardDrive size={15} />
          <span><b>{c.thisPc}</b><small>{c.protectedOnThisPc}</small></span>
        </div>
        <div>
          <Cloud size={15} />
          <span><b>Google Drive</b><small>{status?.googleDriveConnected ? c.connected : c.notConnected}</small></span>
        </div>
        <div>
          <History size={15} />
          <span><b>{c.lastSync}</b><small>{formatDate(status?.lastSyncAt, locale, c.neverSynced)}</small></span>
        </div>
      </div>

      {status?.quota ? (
        <div className={`cloud-quota-strip is-${status.quota.state}`}>
          <div>
            <span>{c.driveStorage}</span>
            <strong>
              {status.quota.availableBytes == null
                ? `${formatBytes(status.quota.usageBytes)} ${c.used}`
                : `${formatBytes(status.quota.availableBytes)} ${c.available}`}
            </strong>
          </div>
          {quota != null ? (
            <div className="cloud-quota-meter" aria-label={`${quota}% ${c.used}`}>
              <span style={{ width: `${quota}%` }} />
            </div>
          ) : null}
        </div>
      ) : null}

      <div className="cloud-save-main-actions">
        {status?.googleDriveConnected ? (
          <>
            <button type="button" className="primary" onClick={onSync} disabled={!enabled || isBusy || !status?.canSync}>
              <RefreshCcw size={15} className={isBusy ? 'spin' : ''} />
              {isBusy ? c.checking : c.syncNow}
            </button>
            <button type="button" onClick={onBackupGoogleDrive} disabled={!enabled || isBusy}>
              <CloudUpload size={15} />
              {c.useThisPc}
            </button>
            <button type="button" onClick={onRestoreMissingFiles} disabled={!enabled || isBusy}>
              <CloudDownload size={15} />
              {c.checkCloud}
            </button>
          </>
        ) : (
          <button type="button" className="primary" onClick={onConnectGoogleDrive} disabled={isBusy || !status?.googleDriveConfigured}>
            <LogIn size={15} />
            {c.connectGoogle}
          </button>
        )}
      </div>

      {conflicts.map((conflict) => {
        const recommended = conflict.recommended === 'cloud' ? 'cloud' : 'local'
        return (
          <article className="cloud-conflict" key={conflict.id}>
            <AlertTriangle size={19} />
            <div className="cloud-conflict-body">
              <strong>{c.chooseProgress}</strong>
              <p>{c.bothPreserved} {recommended === 'local' ? c.recommendedLocal : c.recommendedCloud}</p>
              {conflict.recommendationReason ? (
                <p className="cloud-conflict-reason">
                  <ShieldCheck size={14} />
                  {conflict.recommendationReason}
                </p>
              ) : null}
              <div className="cloud-conflict-compare">
                <span><b>{c.thisPc}</b>{conflict.localDevice || c.thisPc} · {conflict.localFileCount} {c.files} · {formatBytes(conflict.localBytes)}</span>
                <span><b>{c.cloud}</b>{conflict.cloudDevice || 'Google Drive'} · {conflict.cloudFileCount} {c.files} · {formatBytes(conflict.cloudBytes)}</span>
              </div>
              <div className="cloud-conflict-actions">
                <button className={recommended === 'local' ? 'primary' : ''} type="button" onClick={() => onResolve(conflict.id, 'local')} disabled={isBusy}>
                  <HardDrive size={14} />
                  {recommended === 'local' ? c.continueRecommended : c.useLocal}
                </button>
                <button className={recommended === 'cloud' ? 'primary' : ''} type="button" onClick={() => onResolve(conflict.id, 'cloud')} disabled={isBusy}>
                  <Cloud size={14} />
                  {recommended === 'cloud' ? c.continueRecommended : c.useCloud}
                </button>
              </div>
            </div>
          </article>
        )
      })}

      {launchBlocked ? (
        <button className="cloud-launch-anyway" type="button" onClick={onLaunchWithoutSync}>
          {c.playLocalBranch}
        </button>
      ) : null}

      {latestSnapshot ? (
        <div className="cloud-save-latest-backup">
          <Archive size={16} />
          <span>
            <b>{c.latestRecovery}</b>
            <small>{formatDate(latestSnapshot.createdAt, locale, c.neverSynced)} · {latestSnapshot.fileCount} {c.files} · {formatBytes(latestSnapshot.bytes)}</small>
          </span>
          <button type="button" onClick={() => onRestore(latestSnapshot.id)} disabled={isBusy}>
            <RotateCcw size={14} /> {c.restore}
          </button>
        </div>
      ) : null}

      <button className="cloud-save-advanced-toggle" type="button" onClick={() => setAdvancedOpen((value) => !value)} aria-expanded={advancedOpen}>
        <span>{c.advanced}</span>
        <ChevronRight size={16} className={advancedOpen ? 'is-open' : ''} />
      </button>

      {advancedOpen ? (
        <div className="cloud-save-advanced">
          <section>
            <div className="cloud-advanced-title"><Sparkles size={15} /><strong>{c.saveDetection}</strong></div>
            <p>{c.verifiedMap}</p>
            <small>{c.mapLabel} {status?.mapStatus?.version || c.builtIn} · {c.sourceLabel} {status?.mapStatus?.source || c.fallback}</small>
            <div className="cloud-save-roots">
              {roots.map((root) => (
                <span key={`${root.id ?? ''}:${root.path}`} title={root.path} className={root.legacy ? 'is-legacy' : ''}>
                  {root.label || root.path}{root.legacy ? ` · ${c.legacyPath}` : ''}
                </span>
              ))}
              <button type="button" onClick={onAddFolder} disabled={isBusy}>
                <FolderPlus size={14} /> {c.addManually}
              </button>
            </div>
          </section>

          <section>
            <div className="cloud-advanced-title"><History size={15} /><strong>{c.recoveryHistory}</strong></div>
            {snapshots.length ? snapshots.map((snapshot) => (
              <div className="cloud-snapshot-row" key={snapshot.id}>
                <span>
                  <b>{formatDate(snapshot.createdAt, locale, c.neverSynced)}</b>
                  <small>{snapshot.source} · {snapshot.fileCount} {c.files} · {formatBytes(snapshot.bytes)}</small>
                </span>
                <div>
                  <button type="button" title={snapshot.pinned ? c.unpin : c.pinForever} onClick={() => void togglePin(snapshot.id, !snapshot.pinned)} disabled={isBusy}>
                    {snapshot.pinned ? <PinOff size={13} /> : <Pin size={13} />}
                  </button>
                  <button type="button" onClick={() => void exportSnapshot(snapshot.id)} disabled={isBusy}>
                    {c.export}
                  </button>
                  <button type="button" onClick={() => onRestore(snapshot.id)} disabled={isBusy}>
                    {c.restore}
                  </button>
                </div>
              </div>
            )) : <p>{c.noRecovery}</p>}
          </section>

          <footer>
            <button type="button" onClick={() => void exportSnapshot()} disabled={isBusy || !status?.googleDriveConnected}>
              <Archive size={14} /> {c.exportCurrentCloud}
            </button>
            {status?.googleDriveConnected ? (
              <button type="button" onClick={onDisconnectGoogleDrive} disabled={isBusy}>
                <LogOut size={14} /> {c.disconnectGoogle}
              </button>
            ) : null}
          </footer>
        </div>
      ) : null}
    </section>
  )
}
