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

function formatDate(value?: string | null) {
  if (!value) return 'Chưa đồng bộ'
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString('vi-VN')
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
  const enabled = status?.enabled ?? false
  const roots = status?.saveRoots ?? []
  const conflicts = status?.conflicts ?? []
  const snapshots = status?.snapshots ?? []
  const presentation = cloudSavePresentation(status)
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
      title: 'Chọn thư mục xuất bản sao lưu',
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
            <strong>Bảo vệ tự động</strong>
            <small>Google Drive · vùng dữ liệu riêng của launcher</small>
          </div>
        </div>
        <button
          className={enabled ? 'cloud-save-toggle is-on' : 'cloud-save-toggle'}
          type="button"
          role="switch"
          aria-checked={enabled}
          aria-label="Bật hoặc tắt bảo vệ Cloud Save"
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
          <p>{status?.lastMessage || presentation.description}</p>
        </div>
        {status?.pendingOperationCount ? (
          <span className="cloud-save-pending-chip">
            Đang chờ đồng bộ · {formatBytes(status.pendingUploadBytes)}
          </span>
        ) : null}
      </div>

      <div className="cloud-save-facts">
        <div>
          <HardDrive size={15} />
          <span><b>Trên máy này</b><small>Được bảo vệ trên máy này</small></span>
        </div>
        <div>
          <Cloud size={15} />
          <span><b>Google Drive</b><small>{status?.googleDriveConnected ? 'Đã kết nối' : 'Chưa kết nối'}</small></span>
        </div>
        <div>
          <History size={15} />
          <span><b>Lần gần nhất</b><small>{formatDate(status?.lastSyncAt)}</small></span>
        </div>
      </div>

      {status?.quota ? (
        <div className={`cloud-quota-strip is-${status.quota.state}`}>
          <div>
            <span>Dung lượng Google Drive</span>
            <strong>
              {status.quota.availableBytes == null
                ? `${formatBytes(status.quota.usageBytes)} đã dùng`
                : `${formatBytes(status.quota.availableBytes)} còn trống`}
            </strong>
          </div>
          {quota != null ? (
            <div className="cloud-quota-meter" aria-label={`Đã dùng ${quota}% dung lượng Google Drive`}>
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
              {isBusy ? 'Đang kiểm tra…' : 'Đồng bộ ngay'}
            </button>
            <button type="button" onClick={onBackupGoogleDrive} disabled={!enabled || isBusy}>
              <CloudUpload size={15} />
              Dùng bản trên máy
            </button>
            <button type="button" onClick={onRestoreMissingFiles} disabled={!enabled || isBusy}>
              <CloudDownload size={15} />
              Kiểm tra bản Cloud
            </button>
          </>
        ) : (
          <button type="button" className="primary" onClick={onConnectGoogleDrive} disabled={isBusy || !status?.googleDriveConfigured}>
            <LogIn size={15} />
            Kết nối Google Drive
          </button>
        )}
      </div>

      {conflicts.map((conflict) => {
        const recommended = conflict.recommended === 'cloud' ? 'cloud' : 'local'
        return (
          <article className="cloud-conflict" key={conflict.id}>
            <AlertTriangle size={19} />
            <div className="cloud-conflict-body">
              <strong>Cần chọn bản tiến trình</strong>
              <p>Cả hai bản đã được sao lưu an toàn. Launcher đề xuất bản {recommended === 'local' ? 'trên máy này' : 'trên Cloud'}.</p>
              {conflict.recommendationReason ? (
                <p className="cloud-conflict-reason">
                  <ShieldCheck size={14} />
                  {conflict.recommendationReason}
                </p>
              ) : null}
              <div className="cloud-conflict-compare">
                <span><b>Máy này</b>{conflict.localDevice || 'This PC'} · {conflict.localFileCount} file · {formatBytes(conflict.localBytes)}</span>
                <span><b>Cloud</b>{conflict.cloudDevice || 'Google Drive'} · {conflict.cloudFileCount} file · {formatBytes(conflict.cloudBytes)}</span>
              </div>
              <div className="cloud-conflict-actions">
                <button className={recommended === 'local' ? 'primary' : ''} type="button" onClick={() => onResolve(conflict.id, 'local')} disabled={isBusy}>
                  <HardDrive size={14} />
                  {recommended === 'local' ? 'Tiếp tục với bản đề xuất' : 'Dùng bản trên máy'}
                </button>
                <button className={recommended === 'cloud' ? 'primary' : ''} type="button" onClick={() => onResolve(conflict.id, 'cloud')} disabled={isBusy}>
                  <Cloud size={14} />
                  {recommended === 'cloud' ? 'Tiếp tục với bản đề xuất' : 'Dùng bản trên Cloud'}
                </button>
              </div>
            </div>
          </article>
        )
      })}

      {launchBlocked ? (
        <button className="cloud-launch-anyway" type="button" onClick={onLaunchWithoutSync}>
          Vẫn chơi bằng bản trên máy — launcher sẽ giữ thành một nhánh riêng
        </button>
      ) : null}

      {latestSnapshot ? (
        <div className="cloud-save-latest-backup">
          <Archive size={16} />
          <span>
            <b>Bản khôi phục gần nhất</b>
            <small>{formatDate(latestSnapshot.createdAt)} · {latestSnapshot.fileCount} file · {formatBytes(latestSnapshot.bytes)}</small>
          </span>
          <button type="button" onClick={() => onRestore(latestSnapshot.id)} disabled={isBusy}>
            <RotateCcw size={14} /> Khôi phục
          </button>
        </div>
      ) : null}

      <button className="cloud-save-advanced-toggle" type="button" onClick={() => setAdvancedOpen((value) => !value)} aria-expanded={advancedOpen}>
        <span>Xem chi tiết nâng cao</span>
        <ChevronRight size={16} className={advancedOpen ? 'is-open' : ''} />
      </button>

      {advancedOpen ? (
        <div className="cloud-save-advanced">
          <section>
            <div className="cloud-advanced-title"><Sparkles size={15} /><strong>Nhận diện Save</strong></div>
            <p>{status?.mapStatus?.message || 'Launcher đang dùng save-path map đã xác minh.'}</p>
            <small>Map {status?.mapStatus?.version || 'built-in'} · {status?.mapStatus?.source || 'fallback'}</small>
            <div className="cloud-save-roots">
              {roots.map((root) => (
                <span key={`${root.id ?? ''}:${root.path}`} title={root.path} className={root.legacy ? 'is-legacy' : ''}>
                  {root.label || root.path}{root.legacy ? ' · đường dẫn cũ được giữ an toàn' : ''}
                </span>
              ))}
              <button type="button" onClick={onAddFolder} disabled={isBusy}>
                <FolderPlus size={14} /> Thêm thủ công
              </button>
            </div>
          </section>

          <section>
            <div className="cloud-advanced-title"><History size={15} /><strong>Lịch sử khôi phục</strong></div>
            {snapshots.length ? snapshots.map((snapshot) => (
              <div className="cloud-snapshot-row" key={snapshot.id}>
                <span>
                  <b>{formatDate(snapshot.createdAt)}</b>
                  <small>{snapshot.source} · {snapshot.fileCount} file · {formatBytes(snapshot.bytes)}</small>
                </span>
                <div>
                  <button type="button" title={snapshot.pinned ? 'Bỏ ghim' : 'Giữ vô thời hạn'} onClick={() => void togglePin(snapshot.id, !snapshot.pinned)} disabled={isBusy}>
                    {snapshot.pinned ? <PinOff size={13} /> : <Pin size={13} />}
                  </button>
                  <button type="button" onClick={() => void exportSnapshot(snapshot.id)} disabled={isBusy}>
                    Xuất
                  </button>
                  <button type="button" onClick={() => onRestore(snapshot.id)} disabled={isBusy}>
                    Khôi phục
                  </button>
                </div>
              </div>
            )) : <p>Chưa có bản khôi phục. Bản đầu tiên sẽ được tạo tự động.</p>}
          </section>

          <footer>
            <button type="button" onClick={() => void exportSnapshot()} disabled={isBusy || !status?.googleDriveConnected}>
              <Archive size={14} /> Xuất bản Cloud hiện tại
            </button>
            {status?.googleDriveConnected ? (
              <button type="button" onClick={onDisconnectGoogleDrive} disabled={isBusy}>
                <LogOut size={14} /> Ngắt kết nối Google Drive
              </button>
            ) : null}
          </footer>
        </div>
      ) : null}
    </section>
  )
}
