import { AlertTriangle, Cloud, CloudDownload, CloudUpload, FolderPlus, LogIn, LogOut, RefreshCcw, RotateCcw, UploadCloud } from 'lucide-react'
import type { CloudSaveStatus } from '../types'
import { formatBytes } from '../lib/format'

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
  const enabled = status?.enabled ?? false
  const roots = status?.saveRoots ?? []
  const conflicts = status?.conflicts ?? []
  const snapshots = status?.snapshots ?? []

  return (
    <section className={`panel cloud-save-panel${conflicts.length ? ' has-conflict' : ''}`}>
      <header className="side-header">
        <Cloud size={17} />
        <strong>CLOUD SAVE</strong>
        <button
          className={enabled ? 'cloud-save-toggle is-on' : 'cloud-save-toggle'}
          type="button"
          role="switch"
          aria-checked={enabled}
          aria-label="Enable cloud save"
          disabled={busy}
          onClick={() => onToggle(!enabled)}
        >
          <span />
        </button>
      </header>

      <p className="cloud-save-message">
        {status?.lastMessage || 'Cloud save is disabled for this game.'}
      </p>

      <div className="cloud-save-roots">
        {roots.map((root) => (
          <span key={root.path} title={root.path}>{root.label || root.path}</span>
        ))}
        <button type="button" onClick={onAddFolder} disabled={busy}>
          <FolderPlus size={14} />
          Add save folder
        </button>
      </div>

      <div className="cloud-save-actions">
        <button
          type="button"
          onClick={onSync}
          disabled={!enabled || busy || !status?.canSync || roots.length === 0}
        >
          <RefreshCcw size={14} />
          {busy ? 'Syncing...' : 'Sync now'}
        </button>
        {status?.lastSyncAt ? <small>Last sync: {new Date(status.lastSyncAt).toLocaleString()}</small> : null}
      </div>

      <div className="google-drive-backup">
        <div>
          <strong>Google Drive backup</strong>
          <small>
            {status?.googleDriveMessage ||
              (status?.googleDriveConnected
                ? 'Connected. Missing files are restored automatically before launch.'
                : 'Sign in once, then back up this game’s save files.')}
          </small>
        </div>
        <div className="google-drive-actions">
          {status?.googleDriveConnected ? (
            <>
              <button type="button" onClick={onBackupGoogleDrive} disabled={busy}>
                <CloudUpload size={14} />
                Backup save game
              </button>
              <button type="button" onClick={onRestoreMissingFiles} disabled={busy}>
                <CloudDownload size={14} />
                Restore missing
              </button>
              <button type="button" onClick={onDisconnectGoogleDrive} disabled={busy}>
                <LogOut size={14} />
                Disconnect
              </button>
            </>
          ) : (
            <button
              type="button"
              onClick={onConnectGoogleDrive}
              disabled={busy || !status?.googleDriveConfigured}
              title={
                status?.googleDriveConfigured
                  ? 'Sign in with Google and immediately back up this game'
                  : 'Configure the Google OAuth client ID in Settings first'
              }
            >
              <LogIn size={14} />
              Sign in &amp; back up
            </button>
          )}
        </div>
        {status?.googleDriveLastBackupAt ? (
          <small>Last Google Drive backup: {new Date(status.googleDriveLastBackupAt).toLocaleString()}</small>
        ) : null}
      </div>

      {conflicts.map((conflict) => (
        <article className="cloud-conflict" key={conflict.id}>
          <AlertTriangle size={17} />
          <div>
            <strong>Save conflict</strong>
            <small>
              Local {conflict.localFileCount} files ({formatBytes(conflict.localBytes)}) · Cloud{' '}
              {conflict.cloudFileCount} files ({formatBytes(conflict.cloudBytes)})
            </small>
            <div>
              <button type="button" onClick={() => onResolve(conflict.id, 'local')} disabled={busy}>
                <UploadCloud size={13} />
                Use local
              </button>
              <button type="button" onClick={() => onResolve(conflict.id, 'cloud')} disabled={busy}>
                <Cloud size={13} />
                Use cloud
              </button>
            </div>
          </div>
        </article>
      ))}

      {launchBlocked ? (
        <button className="cloud-launch-anyway" type="button" onClick={onLaunchWithoutSync}>
          Launch without sync
        </button>
      ) : null}

      {snapshots.length > 0 ? (
        <details className="cloud-snapshots">
          <summary>Restore points ({snapshots.length})</summary>
          {snapshots.map((snapshot) => (
            <div key={snapshot.id}>
              <span>
                {snapshot.source} · {snapshot.fileCount} files · {formatBytes(snapshot.bytes)}
              </span>
              <button type="button" onClick={() => onRestore(snapshot.id)} disabled={busy}>
                <RotateCcw size={13} />
                Restore
              </button>
            </div>
          ))}
        </details>
      ) : null}
    </section>
  )
}
