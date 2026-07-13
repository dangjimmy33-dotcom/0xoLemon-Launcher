import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Check, CheckCircle2, ChevronDown, Download, FolderOpen, Gauge, HardDrive, Plus, X } from 'lucide-react'
import { enUS as t } from '../i18n/en-US'
import type { GameDetail, GameVersionInfo } from '../types'
import { formatBytes } from '../lib/format'

export function InstallBar({
  installPath,
  installTarget,
  scanStatus,
  installMode,
  onBrowse,
  onScan,
}: {
  installPath: string
  installTarget: string
  scanStatus: string
  installMode: boolean
  onBrowse: () => void
  onScan: () => void
}) {
  const label = installMode ? 'Install target' : 'Installed folder'
  const path = installMode ? installTarget : installPath || 'No installed folder selected'

  return (
    <section className={installMode ? 'install-bar install-mode' : 'install-bar'}>
      <div className="install-path">
        <FolderOpen size={18} />
        <div>
          <small>{label}</small>
          <span>{path}</span>
        </div>
      </div>
      <span className="scan-status">{scanStatus}</span>
      {!installMode ? (
        <>
          <button type="button" onClick={onScan} disabled={!installPath}>
            <Gauge size={16} />
            Scan
          </button>
          <button type="button" onClick={onBrowse}>
            <FolderOpen size={16} />
            Browse
          </button>
        </>
      ) : null}
    </section>
  )
}

export function InstallOptionsDialog({
  detail,
  mode,
  currentVersion,
  selectedVersion,
  availableVersions,
  versionInfos,
  downloadSize,
  installRoot,
  downloadingRoot,
  canStart,
  isStarting = false,
  statusMessage,
  onVersionChange,
  onChangeInstallRoot,
  onStart,
  onClose,
}: {
  detail: GameDetail
  mode: 'install' | 'version'
  currentVersion: string
  selectedVersion: string
  availableVersions: string[]
  versionInfos: GameVersionInfo[]
  downloadSize: number
  installRoot: string
  downloadingRoot: string
  canStart: boolean
  isStarting?: boolean
  statusMessage?: string
  onVersionChange: (version: string) => void
  onChangeInstallRoot: () => void
  onStart: () => void
  onClose: () => void
}) {
  const [versionMenuOpen, setVersionMenuOpen] = useState(false)
  const [diskCheck, setDiskCheck] = useState<{
    has_space: boolean
    free_space: number
    required_space: number
    reason: string | null
  } | null>(null)

  // Check disk space when dialog opens, game changes, or install path changes
  useEffect(() => {
    if (!installRoot) {
      setDiskCheck(null)
      return
    }

    // Reset immediately to prevent showing stale data from previous drive
    setDiskCheck(null)

    const bufferBytes = 2 * 1024 * 1024 * 1024  // 2GB
    const requiredBytes = downloadSize + bufferBytes

    let cancelled = false

    invoke<{
      has_space: boolean
      free_space: number
      required_space: number
      reason: string | null
    }>('check_install_disk_space', {
      installPath: installRoot,  // camelCase for Tauri
      requiredSizeBytes: requiredBytes
    })
      .then((result) => {
        if (!cancelled) setDiskCheck(result)
      })
      .catch((err) => {
        if (!cancelled) {
          setDiskCheck({
            has_space: false,
            free_space: 0,
            required_space: requiredBytes,
            reason: 'Failed to check disk space: ' + err
          })
        }
      })

    return () => {
      cancelled = true
    }
  }, [detail.gameId, installRoot, downloadSize])

  // Check if we're waiting for disk space result
  const diskCheckPending = Boolean(installRoot) && diskCheck === null

  const infos =
    versionInfos.length > 0
      ? versionInfos
      : availableVersions.map((version) => ({
        version,
        label: version,
        buildId: version,
        sizeBytes: downloadSize,
        latest: version === availableVersions[availableVersions.length - 1],
      }))
  const selectedInfo = infos.find((info) => info.version === selectedVersion) ?? infos[0]

  const isVersionChange = mode === 'version'
  const selectedBuildId = selectedInfo?.buildId?.trim()
  const selectedVersionLabel = selectedInfo?.label || selectedVersion

  return (
    <div className="dialog-backdrop" role="presentation">
      <section className="install-modal" role="dialog" aria-modal="true" aria-labelledby="install-options-title">
        <div className="modal-handle" />
        <header>
          <button type="button" onClick={onClose} aria-label="Close install options">
            <X size={17} />
          </button>
          <h2 id="install-options-title">{isVersionChange ? 'Choose game version' : t.install.title}</h2>
          <p>
            {isVersionChange
              ? 'Select any published version. Choosing an older version will downgrade the installed game.'
              : t.install.subtitle}
          </p>
        </header>
        <div className="install-modal-body">
          <div className={versionMenuOpen ? 'version-dropdown open' : 'version-dropdown'}>
            <small>{t.install.version}</small>
            <button
              className="version-dropdown-trigger"
              type="button"
              aria-haspopup="listbox"
              aria-expanded={versionMenuOpen}
              onClick={() => setVersionMenuOpen((open) => !open)}
            >
              <span>
                <strong>{selectedVersionLabel}</strong>
                <small>{selectedBuildId ? `Build ${selectedBuildId}` : selectedVersion}</small>
              </span>
              {selectedInfo?.latest ? <em>{t.install.latest}</em> : null}
              <ChevronDown size={17} />
            </button>
            {versionMenuOpen ? (
              <div className="version-dropdown-menu" role="listbox" aria-label="Choose install version">
                {infos.map((info) => (
                  <button
                    className={info.version === selectedVersion ? 'version-dropdown-option active' : 'version-dropdown-option'}
                    key={info.version}
                    type="button"
                    role="option"
                    aria-selected={info.version === selectedVersion}
                    onClick={() => {
                      onVersionChange(info.version)
                      setVersionMenuOpen(false)
                    }}
                  >
                    <CheckCircle2 size={17} />
                    <span>
                      <strong>{info.label || info.version}</strong>
                      <small>{info.buildId ? `Build ${info.buildId}` : info.version}</small>
                    </span>
                    {info.latest ? <em>{t.install.latest}</em> : null}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
          <div className="install-options-grid">
            <div>
              <small>{t.install.version}</small>
              <strong>{selectedVersionLabel}{selectedBuildId ? ` (Build ${selectedBuildId})` : ''}</strong>
            </div>
            {isVersionChange ? (
              <div>
                <small>Currently installed</small>
                <strong>{currentVersion}</strong>
              </div>
            ) : null}
            <div>
              <small>{t.install.downloadSize}</small>
              <strong>{formatBytes(downloadSize)}</strong>
            </div>
            {diskCheck && (
              <div>
                <small>Available disk space</small>
                <strong style={{ color: diskCheck.has_space ? '#4ade80' : '#ff4444' }}>
                  {formatBytes(diskCheck.required_space)} required / {formatBytes(diskCheck.free_space)} available
                </strong>
              </div>
            )}
            <div>
              <small>{t.install.resumeBehavior}</small>
              <strong>{t.install.journalCache}</strong>
            </div>
            <div>
              <small>Game</small>
              <strong>{detail.title}</strong>
            </div>
            <div className="wide-option">
              <small>{t.install.installFolder}</small>
              <strong>{installRoot}</strong>
              {!isVersionChange ? (
                <button type="button" onClick={onChangeInstallRoot}>
                  <FolderOpen size={16} />
                  {t.install.change}
                </button>
              ) : null}
            </div>
            <div className="wide-option">
              <small>{t.install.downloadingFolder}</small>
              <strong>{downloadingRoot}</strong>
            </div>
          </div>
          {/* Disk space warning */}
          {diskCheck && !diskCheck.has_space && (
            <div className="install-modal-status" role="alert" style={{
              color: '#ff4444',
              backgroundColor: 'rgba(255, 68, 68, 0.1)',
              padding: '12px',
              borderRadius: '8px',
              marginTop: '16px',
              border: '1px solid rgba(255, 68, 68, 0.3)'
            }}>
              ⚠️ {diskCheck.reason}
            </div>
          )}
          {statusMessage ? (
            <div className="install-modal-status" role="status" aria-live="polite">
              {statusMessage}
            </div>
          ) : null}
        </div>
        <footer>
          <button type="button" onClick={onClose}>
            {t.install.cancel}
          </button>
          <button className="primary-control" type="button" onClick={onStart} disabled={!canStart || isStarting || diskCheckPending || (diskCheck ? !diskCheck.has_space : false)}>
            {isStarting ? (
              <>
                <svg className="btn-spinner" viewBox="0 0 24 24" width="17" height="17" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
                  <circle cx="12" cy="12" r="9" strokeDasharray="40 20" />
                </svg>
                Starting…
              </>
            ) : diskCheckPending ? (
              <>
                <svg className="btn-spinner" viewBox="0 0 24 24" width="17" height="17" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
                  <circle cx="12" cy="12" r="9" strokeDasharray="40 20" />
                </svg>
                Checking disk space…
              </>
            ) : (diskCheck && !diskCheck.has_space) ? (
              <>
                <X size={17} />
                Insufficient disk space
              </>
            ) : (
              <>
                <Download size={17} />
                {isVersionChange
                  ? canStart
                    ? 'Apply selected version'
                    : 'Current version selected'
                  : t.install.startDownload}
              </>
            )}
          </button>
        </footer>
      </section>
    </div>
  )
}

export function DriveLibraryPickerModal({
  libraries,
  gameName,
  currentRoot,
  onSelect,
  onAddDrive,
  onClose,
}: {
  libraries: string[]
  gameName: string
  currentRoot: string
  onSelect: (driveLetter: string) => void
  onAddDrive: () => void
  onClose: () => void
}) {
  type DriveInfo = { letter: string; label: string; free_bytes: number; total_bytes: number }
  const [driveInfos, setDriveInfos] = useState<Record<string, DriveInfo>>({})
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    setLoading(true)
    invoke<DriveInfo[]>('list_system_drives')
      .then((drives) => {
        const map: Record<string, DriveInfo> = {}
        // Normalize: backend returns "E:", but libraries might be "E:\" or "E:"
        for (const d of drives) {
          map[d.letter.toUpperCase()] = d
          map[d.letter.toUpperCase() + '\\'] = d  // Also map "E:\" variant
        }
        setDriveInfos(map)
        setLoading(false)
      })
      .catch(() => {
        setLoading(false)
      })
  }, [])

  if (loading) {
    return (
      <div className="dialog-backdrop" role="presentation">
        <section className="drive-picker-modal" role="dialog" aria-modal="true">
          <header>
            <h2>Loading...</h2>
            <button type="button" onClick={onClose}><X size={17} /></button>
          </header>
        </section>
      </div>
    )
  }

  return (
    <div className="dialog-backdrop" role="presentation" onClick={(e) => { if (e.target === e.currentTarget) onClose() }}>
      <section className="drive-picker-modal" role="dialog" aria-modal="true" aria-label="Choose install library">
        <header>
          <h2>Choose Install Location</h2>
          <button type="button" onClick={onClose} aria-label="Close"><X size={17} /></button>
        </header>
        <p className="drive-picker-hint">
          Game will be installed to: <code>Drive:\0xoLemon store\common\{gameName}</code>
        </p>
        <div className="drive-list">
          {libraries.map((lib) => {
            const info = driveInfos[lib.toUpperCase()]
            const isSelected = currentRoot.toUpperCase().startsWith(lib.toUpperCase())
            const freeGB = info ? (info.free_bytes / 1024 / 1024 / 1024).toFixed(1) : null
            const totalGB = info ? (info.total_bytes / 1024 / 1024 / 1024).toFixed(0) : null
            const usedPct = info ? Math.round(((info.total_bytes - info.free_bytes) / info.total_bytes) * 100) : 0

            return (
              <button
                key={lib}
                className={`drive-entry${isSelected ? ' selected' : ''}`}
                type="button"
                onClick={() => onSelect(lib)}
              >
                <div className="drive-icon">
                  <HardDrive size={28} />
                </div>
                <div className="drive-details">
                  <div className="drive-label">
                    <strong>{lib}</strong>
                    {info ? <span>{info.label}</span> : null}
                  </div>
                  {info ? (
                    <>
                      <div className="drive-space-bar">
                        <div className="drive-space-fill" style={{ width: `${usedPct}%` }} />
                      </div>
                      <div className="drive-space-text">
                        {freeGB} GB free of {totalGB} GB
                      </div>
                    </>
                  ) : (
                    <>
                      <div className="drive-space-bar">
                        <div className="drive-space-fill" style={{ width: '0%' }} />
                      </div>
                      <div className="drive-space-text muted">—</div>
                    </>
                  )}
                  <div className="drive-path-preview">
                    {lib}\0xoLemon store\common\{gameName}
                  </div>
                </div>
                {isSelected && <div className="drive-check"><Check size={16} /></div>}
              </button>
            )
          })}
        </div>
        <footer>
          <button type="button" className="add-drive-btn" onClick={onAddDrive}>
            <Plus size={15} /> Add Drive
          </button>
        </footer>
      </section>
    </div>
  )
}
