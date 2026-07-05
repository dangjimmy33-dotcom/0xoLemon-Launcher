import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Cloud, CloudUpload, HardDrive, Check, X, Loader2, AlertCircle } from 'lucide-react'

type CloudProvider = 'google_drive' | 'onedrive' | 'local' | null

interface CloudRedirectStatus {
  enabled: boolean
  provider: CloudProvider
  authenticated: boolean
  syncActive: boolean
  lastSync?: string
  error?: string
  autoCloudGames: string[]
}

interface SyncStatus {
  isSyncing: boolean
  currentFile?: string
  progress: number
  filesUploaded: number
  filesDownloaded: number
  bytesTransferred: number
  error?: string
}

interface GameSaveInfo {
  appId: string
  gameName: string
  hasAutoCloud: boolean
  savePath?: string
  saveSize: number
  lastModified?: string
}

interface BackupInfo {
  id: string
  name: string
  appId?: string
  location: string // "local" or "google_drive"
  size: number
  createdAt?: string
  cloudId?: string
}

export function CloudRedirectSettings() {
  const [status, setStatus] = useState<CloudRedirectStatus>({
    enabled: false,
    provider: null,
    authenticated: false,
    syncActive: false,
    autoCloudGames: [],
  })
  const [loading, setLoading] = useState(false)
  const [selectedProvider, setSelectedProvider] = useState<CloudProvider>(null)
  const [localPath, setLocalPath] = useState('')
  const [authInProgress, setAuthInProgress] = useState(false)
  const [authUrlString, setAuthUrlString] = useState('')
  const [syncStatus, setSyncStatus] = useState<SyncStatus>({
    isSyncing: false,
    progress: 0,
    filesUploaded: 0,
    filesDownloaded: 0,
    bytesTransferred: 0,
  })
  const [gameSaves, setGameSaves] = useState<GameSaveInfo[]>([])
  const [backups, setBackups] = useState<BackupInfo[]>([])
  const [showBackupDialog, setShowBackupDialog] = useState(false)
  const [showResetDialog, setShowResetDialog] = useState(false)
  const [showRestoreDialog, setShowRestoreDialog] = useState(false)
  const [selectedGame, setSelectedGame] = useState<GameSaveInfo | null>(null)
  const [selectedBackup, setSelectedBackup] = useState<BackupInfo | null>(null)
  const [uploadToCloud, setUploadToCloud] = useState(true)
  const [luaGameModeEnabled, setLuaGameModeEnabled] = useState<boolean | null>(null)

  useEffect(() => {
    checkLuaGameMode()
    checkStatus()
    loadGameSaves()
    loadBackups()
  }, [])

  const checkLuaGameMode = async () => {
    try {
      const isEnabled = await invoke<boolean>('is_lua_game_mode_enabled')
      setLuaGameModeEnabled(isEnabled)
    } catch (e) {
      console.error('Failed to check Lua-Game Mode status', e)
      setLuaGameModeEnabled(false)
    }
  }

  const navigateToSettings = () => {
    // Navigate to Settings tab
    window.dispatchEvent(new CustomEvent('navigate-to-tab', { detail: 'Settings' }))
  }

  // Poll sync status when enabled
  useEffect(() => {
    if (!status.enabled) return

    const pollSyncStatus = async () => {
      try {
        const result = await invoke<SyncStatus>('cloud_redirect_get_sync_status')
        setSyncStatus(result)
      } catch (e) {
        console.error('Failed to get sync status', e)
      }
    }

    // Poll every 2 seconds
    const interval = setInterval(pollSyncStatus, 2000)
    pollSyncStatus() // Initial call

    return () => clearInterval(interval)
  }, [status.enabled])

  const checkStatus = async () => {
    try {
      const result = await invoke<CloudRedirectStatus>('cloud_redirect_v2_get_status')
      setStatus(result)
      setSelectedProvider(result.provider)
    } catch (e) {
      console.error('Failed to get CloudRedirect status', e)
    }
  }

  const loadGameSaves = async () => {
    try {
      const saves = await invoke<GameSaveInfo[]>('cloud_redirect_list_game_saves')
      setGameSaves(saves)
    } catch (e) {
      console.error('Failed to load game saves', e)
    }
  }

  const loadBackups = async () => {
    try {
      const backupList = await invoke<BackupInfo[]>('cloud_redirect_list_backups', { appId: null })
      setBackups(backupList)
    } catch (e) {
      console.error('Failed to load backups', e)
    }
  }

  const handleBackupSave = async (game: GameSaveInfo) => {
    setLoading(true)
    try {
      const result = await invoke<string>('cloud_redirect_backup_save', {
        appId: game.appId,
        uploadToCloud
      })
      showToast('Success', result, 'success')
      await loadBackups()
    } catch (e) {
      showToast('Error', String(e), 'error')
    }
    setLoading(false)
    setShowBackupDialog(false)
  }

  const handleRestoreBackup = async (backup: BackupInfo) => {
    setLoading(true)
    try {
      await invoke('cloud_redirect_restore_backup', {
        backupId: backup.id,
        location: backup.location
      })
      showToast('Success', `Restored from ${backup.name}`, 'success')
      await loadGameSaves()
    } catch (e) {
      showToast('Error', String(e), 'error')
    }
    setLoading(false)
    setShowRestoreDialog(false)
  }

  const handleResetGame = async (game: GameSaveInfo) => {
    setLoading(true)
    try {
      await invoke('cloud_redirect_reset_game', { appId: game.appId })
      showToast('Success', `Progress reset for ${game.gameName}`, 'success')
      await loadGameSaves()
    } catch (e) {
      showToast('Error', String(e), 'error')
    }
    setLoading(false)
    setShowResetDialog(false)
  }

  const handleProviderChange = (provider: CloudProvider) => {
    setSelectedProvider(provider)
  }

  const handleAuthenticate = async () => {
    if (!selectedProvider) return

    setAuthInProgress(true)
    setLoading(true)

    try {
      if (selectedProvider === 'local') {
        // For local provider, just set the path
        if (!localPath) {
          showToast('Error', 'Please enter a local folder path', 'error')
          return
        }
        await invoke('cloud_redirect_set_local_path', { path: localPath })
        showToast('Success', 'Local folder configured', 'success')
        await checkStatus()
      } else {
        // For cloud providers, start OAuth flow with callback server
        const authUrl = await invoke<string>('cloud_redirect_start_oauth', {
          provider: selectedProvider
        })

        // Open OAuth URL in browser
        setAuthUrlString(authUrl)
        await invoke('open_url', { url: authUrl })

        showToast('Authentication', 'Please complete the sign-in in your browser', 'info')

        // Poll for OAuth code from callback server
        let attempts = 0
        const pollInterval = setInterval(async () => {
          attempts++
          if (attempts > 60) { // 60 seconds timeout
            clearInterval(pollInterval)
            setAuthInProgress(false)
            setLoading(false)
            showToast('Timeout', 'Authentication timed out', 'error')
            return
          }

          try {
            const code = await invoke<string | null>('cloud_redirect_poll_oauth_code')
            if (code) {
              clearInterval(pollInterval)

              // Exchange code for tokens
              await invoke('cloud_redirect_complete_oauth', {
                provider: selectedProvider,
                code: code
              })

              await checkStatus()
              setAuthInProgress(false)
              setLoading(false)
              showToast('Success', 'Authentication successful!', 'success')
            }
          } catch (e) {
            // Continue polling
          }
        }, 1000)
      }
    } catch (e) {
      console.error(e)
      showToast('Error', String(e), 'error')
      setAuthInProgress(false)
      setLoading(false)
    } finally {
      if (selectedProvider === 'local') {
        setAuthInProgress(false)
        setLoading(false)
      }
    }
  }

  const handleEnable = async () => {
    setLoading(true)
    try {
      await invoke('cloud_redirect_enable')
      await checkStatus()
      showToast('Success', 'CloudRedirect enabled', 'success')
    } catch (e) {
      console.error(e)
      showToast('Error', String(e), 'error')
    }
    setLoading(false)
  }

  const handleDisable = async () => {
    setLoading(true)
    try {
      await invoke('cloud_redirect_disable')
      await checkStatus()
      showToast('Success', 'CloudRedirect disabled', 'success')
    } catch (e) {
      console.error(e)
      showToast('Error', String(e), 'error')
    }
    setLoading(false)
  }

  const handleBrowse = async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select CloudRedirect folder',
      })
      if (selected && typeof selected === 'string') {
        setLocalPath(selected)
      }
    } catch (e) {
      console.error(e)
    }
  }

  const showToast = (title: string, msg: string, severity: 'success' | 'error' | 'info' | 'warning' = 'info') => {
    window.dispatchEvent(new CustomEvent('0xo-toast', {
      detail: {
        category: 'launcher',
        severity,
        title,
        message: msg,
        dedupeKey: 'cloud-redirect',
      }
    }))
  }

  return (
    <div style={{
      marginTop: '16px',
      padding: '20px',
      background: 'rgba(0,149,255,0.05)',
      border: '1px solid rgba(0,149,255,0.2)',
      borderRadius: '12px',
      position: 'relative',
    }}>
      {/* Lua-Game Mode Required Overlay */}
      {luaGameModeEnabled === false && (
        <div style={{
          position: 'absolute',
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          background: 'rgba(0,0,0,0.85)',
          backdropFilter: 'blur(8px)',
          borderRadius: '12px',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          gap: '16px',
          zIndex: 10,
          padding: '40px',
        }}>
          <AlertCircle size={48} style={{ color: '#ffa726' }} />
          <div style={{ textAlign: 'center' }}>
            <h3 style={{ color: '#ffa726', fontSize: '18px', marginBottom: '8px' }}>
              Lua-Game Mode Required
            </h3>
            <p style={{ color: '#ccc', fontSize: '14px', maxWidth: '400px', lineHeight: '1.6' }}>
              CloudRedirect requires Lua-Game Mode to be enabled.
              Please enable Lua-Game Mode in Settings first.
            </p>
          </div>
          <button
            onClick={navigateToSettings}
            style={{
              padding: '12px 24px',
              background: 'linear-gradient(135deg, #ffa726, #ff9800)',
              border: 'none',
              borderRadius: '8px',
              color: '#fff',
              fontSize: '14px',
              fontWeight: 600,
              cursor: 'pointer',
              transition: 'all 0.2s',
            }}
            onMouseEnter={e => {
              e.currentTarget.style.transform = 'scale(1.05)'
            }}
            onMouseLeave={e => {
              e.currentTarget.style.transform = 'scale(1)'
            }}
          >
            Go to Settings
          </button>
        </div>
      )}

      {/* Main Content (blurred when Lua-Game Mode not enabled) */}
      <div style={{
        opacity: luaGameModeEnabled === false ? 0.3 : 1,
        pointerEvents: luaGameModeEnabled === false ? 'none' : 'auto',
      }}>
        {/* Header */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '12px', marginBottom: '16px' }}>
          <Cloud size={22} style={{ color: '#0095ff' }} />
          <div style={{ flex: 1 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '12px', marginBottom: '4px' }}>
              <strong style={{ color: '#0095ff', fontSize: '16px' }}>CloudRedirect</strong>
              <div style={{
                padding: '2px 8px',
                background: status.enabled ? 'rgba(0,200,83,0.15)' : 'rgba(255,255,255,0.1)',
                border: `1px solid ${status.enabled ? 'rgba(0,200,83,0.3)' : 'rgba(255,255,255,0.2)'}`,
                borderRadius: '4px',
                fontSize: '11px',
                color: status.enabled ? '#00c853' : '#999',
                fontWeight: 600,
              }}>
                {status.enabled ? 'ENABLED' : 'DISABLED'}
              </div>
            </div>
            <p style={{ fontSize: '13px', color: '#aaa', margin: 0 }}>
              Cloud saves for lua games using Google Drive, OneDrive, or local folder
            </p>
          </div>
        </div>

        {/* Provider Selection */}
        <div style={{ marginBottom: '16px' }}>
          <label style={{
            display: 'block',
            fontSize: '13px',
            fontWeight: 600,
            color: '#ccc',
            marginBottom: '8px'
          }}>
            Cloud Provider
          </label>
          <div style={{ display: 'flex', gap: '12px' }}>
            {/* Google Drive */}
            <button
              style={{
                flex: 1,
                padding: '16px',
                background: selectedProvider === 'google_drive' ? 'rgba(0,149,255,0.15)' : 'rgba(255,255,255,0.05)',
                border: `2px solid ${selectedProvider === 'google_drive' ? '#0095ff' : 'rgba(255,255,255,0.1)'}`,
                borderRadius: '8px',
                cursor: 'pointer',
                transition: 'all 0.2s',
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                gap: '8px',
              }}
              onClick={() => handleProviderChange('google_drive')}
              onMouseEnter={e => {
                if (selectedProvider !== 'google_drive') {
                  e.currentTarget.style.background = 'rgba(255,255,255,0.08)'
                  e.currentTarget.style.borderColor = 'rgba(255,255,255,0.2)'
                }
              }}
              onMouseLeave={e => {
                if (selectedProvider !== 'google_drive') {
                  e.currentTarget.style.background = 'rgba(255,255,255,0.05)'
                  e.currentTarget.style.borderColor = 'rgba(255,255,255,0.1)'
                }
              }}
            >
              <CloudUpload size={24} style={{ color: selectedProvider === 'google_drive' ? '#0095ff' : '#999' }} />
              <span style={{ fontSize: '13px', fontWeight: 600, color: selectedProvider === 'google_drive' ? '#0095ff' : '#ccc' }}>
                Google Drive
              </span>
            </button>

            {/* OneDrive */}
            <button
              style={{
                flex: 1,
                padding: '16px',
                background: selectedProvider === 'onedrive' ? 'rgba(0,149,255,0.15)' : 'rgba(255,255,255,0.05)',
                border: `2px solid ${selectedProvider === 'onedrive' ? '#0095ff' : 'rgba(255,255,255,0.1)'}`,
                borderRadius: '8px',
                cursor: 'pointer',
                transition: 'all 0.2s',
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                gap: '8px',
              }}
              onClick={() => handleProviderChange('onedrive')}
              onMouseEnter={e => {
                if (selectedProvider !== 'onedrive') {
                  e.currentTarget.style.background = 'rgba(255,255,255,0.08)'
                  e.currentTarget.style.borderColor = 'rgba(255,255,255,0.2)'
                }
              }}
              onMouseLeave={e => {
                if (selectedProvider !== 'onedrive') {
                  e.currentTarget.style.background = 'rgba(255,255,255,0.05)'
                  e.currentTarget.style.borderColor = 'rgba(255,255,255,0.1)'
                }
              }}
            >
              <Cloud size={24} style={{ color: selectedProvider === 'onedrive' ? '#0095ff' : '#999' }} />
              <span style={{ fontSize: '13px', fontWeight: 600, color: selectedProvider === 'onedrive' ? '#0095ff' : '#ccc' }}>
                OneDrive
              </span>
            </button>

            {/* Local Folder */}
            <button
              style={{
                flex: 1,
                padding: '16px',
                background: selectedProvider === 'local' ? 'rgba(0,149,255,0.15)' : 'rgba(255,255,255,0.05)',
                border: `2px solid ${selectedProvider === 'local' ? '#0095ff' : 'rgba(255,255,255,0.1)'}`,
                borderRadius: '8px',
                cursor: 'pointer',
                transition: 'all 0.2s',
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                gap: '8px',
              }}
              onClick={() => handleProviderChange('local')}
              onMouseEnter={e => {
                if (selectedProvider !== 'local') {
                  e.currentTarget.style.background = 'rgba(255,255,255,0.08)'
                  e.currentTarget.style.borderColor = 'rgba(255,255,255,0.2)'
                }
              }}
              onMouseLeave={e => {
                if (selectedProvider !== 'local') {
                  e.currentTarget.style.background = 'rgba(255,255,255,0.05)'
                  e.currentTarget.style.borderColor = 'rgba(255,255,255,0.1)'
                }
              }}
            >
              <HardDrive size={24} style={{ color: selectedProvider === 'local' ? '#0095ff' : '#999' }} />
              <span style={{ fontSize: '13px', fontWeight: 600, color: selectedProvider === 'local' ? '#0095ff' : '#ccc' }}>
                Local Folder
              </span>
            </button>
          </div>
        </div>

        {/* Local Path Input */}
        {selectedProvider === 'local' && (
          <div style={{ marginBottom: '16px' }}>
            <label style={{
              display: 'block',
              fontSize: '13px',
              fontWeight: 600,
              color: '#ccc',
              marginBottom: '8px'
            }}>
              Folder Path
            </label>
            <div style={{ display: 'flex', gap: '8px' }}>
              <input
                type="text"
                value={localPath}
                onChange={(e) => setLocalPath(e.target.value)}
                placeholder="C:\CloudSaves"
                style={{
                  flex: 1,
                  padding: '10px 12px',
                  background: 'rgba(255,255,255,0.05)',
                  border: '1px solid rgba(255,255,255,0.1)',
                  borderRadius: '6px',
                  color: '#fff',
                  fontSize: '13px',
                }}
              />
              <button
                onClick={handleBrowse}
                style={{
                  padding: '10px 16px',
                  background: 'rgba(255,255,255,0.1)',
                  border: '1px solid rgba(255,255,255,0.2)',
                  borderRadius: '6px',
                  color: '#fff',
                  fontSize: '13px',
                  fontWeight: 600,
                  cursor: 'pointer',
                }}
              >
                Browse
              </button>
            </div>
          </div>
        )}

        {/* Authentication Status */}
        <div style={{
          padding: '12px 16px',
          background: 'rgba(255,255,255,0.03)',
          border: '1px solid rgba(255,255,255,0.08)',
          borderRadius: '8px',
          marginBottom: '16px',
          display: 'flex',
          alignItems: 'center',
          gap: '12px',
        }}>
          {status.authenticated ? (
            <>
              <Check size={18} style={{ color: '#00c853' }} />
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: '13px', fontWeight: 600, color: '#00c853' }}>Authenticated</div>
                <div style={{ fontSize: '11px', color: '#999' }}>
                  Provider: {status.provider === 'google_drive' ? 'Google Drive' : status.provider === 'onedrive' ? 'OneDrive' : 'Local Folder'}
                </div>
              </div>
            </>
          ) : authInProgress ? (
            <>
              <Loader2 size={18} style={{ color: '#0095ff', animation: 'spin 1s linear infinite', flexShrink: 0 }} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: '13px', fontWeight: 600, color: '#0095ff' }}>Authenticating...</div>
                <div style={{ fontSize: '11px', color: '#999', marginBottom: authUrlString ? '6px' : '0' }}>
                  Complete sign-in in your browser
                </div>
                {authUrlString && (
                  <div style={{
                    fontSize: '11px',
                    background: 'rgba(255,255,255,0.05)',
                    padding: '6px 8px',
                    borderRadius: '4px',
                    border: '1px solid rgba(255,255,255,0.1)',
                    wordBreak: 'break-all',
                    color: '#888'
                  }}>
                    If the browser didn't open, <a href="#" onClick={(e) => { e.preventDefault(); invoke('open_url', { url: authUrlString }) }} style={{ color: '#0095ff', textDecoration: 'none' }}>click here</a> to try again or <a href="#" onClick={(e) => {
                      e.preventDefault()
                      navigator.clipboard.writeText(authUrlString)
                      showToast('Copied', 'URL copied to clipboard', 'success')
                    }} style={{ color: '#0095ff', textDecoration: 'none' }}>copy link</a>.
                  </div>
                )}
              </div>
            </>
          ) : (
            <>
              <AlertCircle size={18} style={{ color: '#999' }} />
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: '13px', fontWeight: 600, color: '#ccc' }}>Not Authenticated</div>
                <div style={{ fontSize: '11px', color: '#999' }}>Sign in to start syncing saves</div>
              </div>
            </>
          )}
        </div>

        {/* Actions */}
        <div style={{ display: 'flex', gap: '12px' }}>
          {!status.authenticated && (
            <button
              onClick={handleAuthenticate}
              disabled={!selectedProvider || loading}
              style={{
                flex: 1,
                padding: '12px',
                background: 'linear-gradient(135deg, #0095ff, #0070cc)',
                border: 'none',
                borderRadius: '8px',
                color: '#fff',
                fontSize: '14px',
                fontWeight: 600,
                cursor: selectedProvider && !loading ? 'pointer' : 'not-allowed',
                opacity: selectedProvider && !loading ? 1 : 0.5,
                transition: 'all 0.2s',
              }}
            >
              {loading ? 'Authenticating...' : 'Sign In'}
            </button>
          )}

          {status.authenticated && (
            <>
              <button
                onClick={status.enabled ? handleDisable : handleEnable}
                disabled={loading}
                style={{
                  flex: 1,
                  padding: '12px',
                  background: status.enabled
                    ? 'rgba(255,255,255,0.1)'
                    : 'linear-gradient(135deg, #0095ff, #0070cc)',
                  border: status.enabled ? '1px solid rgba(255,255,255,0.2)' : 'none',
                  borderRadius: '8px',
                  color: '#fff',
                  fontSize: '14px',
                  fontWeight: 600,
                  cursor: loading ? 'not-allowed' : 'pointer',
                  opacity: loading ? 0.5 : 1,
                  transition: 'all 0.2s',
                }}
              >
                {status.enabled ? 'Disable' : 'Enable'}
              </button>

              <button
                onClick={checkStatus}
                style={{
                  padding: '12px 20px',
                  background: 'rgba(255,255,255,0.1)',
                  border: '1px solid rgba(255,255,255,0.2)',
                  borderRadius: '8px',
                  color: '#fff',
                  fontSize: '14px',
                  fontWeight: 600,
                  cursor: 'pointer',
                }}
              >
                Refresh
              </button>
            </>
          )}
        </div>

        {/* Sync Status */}
        {status.enabled && (
          <div style={{
            marginTop: '16px',
            padding: '12px 16px',
            background: 'rgba(0,149,255,0.08)',
            border: '1px solid rgba(0,149,255,0.2)',
            borderRadius: '8px',
          }}>
            {syncStatus.isSyncing ? (
              <>
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '8px' }}>
                  <Loader2 size={16} style={{ color: '#0095ff', animation: 'spin 1s linear infinite' }} />
                  <span style={{ fontSize: '13px', fontWeight: 600, color: '#0095ff' }}>Syncing...</span>
                </div>
                {syncStatus.currentFile && (
                  <div style={{ fontSize: '11px', color: '#999', marginBottom: '4px' }}>
                    Current: {syncStatus.currentFile}
                  </div>
                )}
                <div style={{
                  width: '100%',
                  height: '4px',
                  background: 'rgba(255,255,255,0.1)',
                  borderRadius: '2px',
                  overflow: 'hidden',
                  marginBottom: '8px',
                }}>
                  <div style={{
                    width: `${syncStatus.progress * 100}%`,
                    height: '100%',
                    background: 'linear-gradient(90deg, #0095ff, #00c6ff)',
                    transition: 'width 0.3s',
                  }} />
                </div>
                <div style={{ display: 'flex', gap: '16px', fontSize: '11px', color: '#999' }}>
                  <span>↑ {syncStatus.filesUploaded} files</span>
                  <span>↓ {syncStatus.filesDownloaded} files</span>
                  <span>{(syncStatus.bytesTransferred / 1024 / 1024).toFixed(2)} MB</span>
                </div>
              </>
            ) : (
              <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                <Check size={16} style={{ color: '#00c853' }} />
                <span style={{ fontSize: '13px', color: '#00c853' }}>Synced</span>
                {status.lastSync && (
                  <span style={{ fontSize: '11px', color: '#999', marginLeft: 'auto' }}>
                    {status.lastSync}
                  </span>
                )}
              </div>
            )}
          </div>
        )}

        {/* Error Display */}
        {status.error && (
          <div style={{
            marginTop: '16px',
            padding: '12px',
            background: 'rgba(255,0,0,0.1)',
            border: '1px solid rgba(255,0,0,0.3)',
            borderRadius: '6px',
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
          }}>
            <X size={16} style={{ color: '#ff4444' }} />
            <span style={{ fontSize: '12px', color: '#ff4444' }}>{status.error}</span>
          </div>
        )}

        {/* AutoCloud Games Detected */}
        {status.autoCloudGames && status.autoCloudGames.length > 0 && (
          <div style={{
            marginTop: '16px',
            padding: '12px 16px',
            background: 'rgba(0,149,255,0.08)',
            border: '1px solid rgba(0,149,255,0.2)',
            borderRadius: '8px',
          }}>
            <div style={{ fontSize: '13px', fontWeight: 600, color: '#0095ff', marginBottom: '8px' }}>
              AutoCloud Games Detected: {status.autoCloudGames.length}
            </div>
            <div style={{ fontSize: '11px', color: '#999' }}>
              These games will automatically sync when CloudRedirect is enabled
            </div>
          </div>
        )}

        {/* Game Saves Management */}
        {status.enabled && gameSaves.length > 0 && (
          <div style={{
            marginTop: '16px',
            padding: '16px',
            background: 'rgba(255,255,255,0.03)',
            border: '1px solid rgba(255,255,255,0.08)',
            borderRadius: '8px',
          }}>
            <div style={{ fontSize: '14px', fontWeight: 600, color: '#ccc', marginBottom: '12px' }}>
              Game Saves Management
            </div>

            <div style={{ maxHeight: '200px', overflowY: 'auto' }}>
              {gameSaves.map((game) => (
                <div key={game.appId} style={{
                  padding: '8px 12px',
                  background: 'rgba(255,255,255,0.05)',
                  border: '1px solid rgba(255,255,255,0.1)',
                  borderRadius: '6px',
                  marginBottom: '8px',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '12px',
                }}>
                  <div style={{ flex: 1 }}>
                    <div style={{ fontSize: '13px', fontWeight: 600, color: '#fff' }}>
                      {game.gameName}
                      {game.hasAutoCloud && (
                        <span style={{
                          marginLeft: '8px',
                          padding: '2px 6px',
                          background: 'rgba(0,149,255,0.2)',
                          border: '1px solid rgba(0,149,255,0.3)',
                          borderRadius: '3px',
                          fontSize: '10px',
                          color: '#0095ff',
                        }}>
                          AutoCloud
                        </span>
                      )}
                    </div>
                    <div style={{ fontSize: '11px', color: '#999', marginTop: '2px' }}>
                      Size: {(game.saveSize / 1024 / 1024).toFixed(2)} MB
                      {game.lastModified && ` • Modified: ${game.lastModified}`}
                    </div>
                  </div>

                  <button
                    onClick={() => {
                      setSelectedGame(game)
                      setShowBackupDialog(true)
                    }}
                    style={{
                      padding: '6px 12px',
                      background: 'rgba(0,149,255,0.15)',
                      border: '1px solid rgba(0,149,255,0.3)',
                      borderRadius: '4px',
                      color: '#0095ff',
                      fontSize: '11px',
                      fontWeight: 600,
                      cursor: 'pointer',
                    }}
                  >
                    Backup
                  </button>

                  <button
                    onClick={() => {
                      setSelectedGame(game)
                      setShowResetDialog(true)
                    }}
                    style={{
                      padding: '6px 12px',
                      background: 'rgba(255,0,0,0.15)',
                      border: '1px solid rgba(255,0,0,0.3)',
                      borderRadius: '4px',
                      color: '#ff4444',
                      fontSize: '11px',
                      fontWeight: 600,
                      cursor: 'pointer',
                    }}
                  >
                    Reset
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Backup Confirmation Dialog */}
        {showBackupDialog && selectedGame && (
          <div style={{
            position: 'fixed',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            background: 'rgba(0,0,0,0.7)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 9999,
          }}>
            <div style={{
              background: '#1a1a1a',
              border: '1px solid rgba(255,255,255,0.1)',
              borderRadius: '12px',
              padding: '24px',
              maxWidth: '400px',
              width: '90%',
            }}>
              <h3 style={{ color: '#fff', marginBottom: '16px' }}>Backup Save</h3>
              <p style={{ color: '#ccc', fontSize: '14px', marginBottom: '16px' }}>
                Create a backup of save data for <strong>{selectedGame.gameName}</strong>?
              </p>

              {status.authenticated && status.provider === 'google_drive' && (
                <label style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '8px',
                  marginBottom: '20px',
                  color: '#ccc',
                  fontSize: '13px',
                  cursor: 'pointer',
                }}>
                  <input
                    type="checkbox"
                    checked={uploadToCloud}
                    onChange={(e) => setUploadToCloud(e.target.checked)}
                    style={{ cursor: 'pointer' }}
                  />
                  Upload to Google Drive
                </label>
              )}

              <div style={{ display: 'flex', gap: '12px' }}>
                <button
                  onClick={() => setShowBackupDialog(false)}
                  style={{
                    flex: 1,
                    padding: '10px',
                    background: 'rgba(255,255,255,0.1)',
                    border: '1px solid rgba(255,255,255,0.2)',
                    borderRadius: '6px',
                    color: '#fff',
                    fontSize: '14px',
                    cursor: 'pointer',
                  }}
                >
                  Cancel
                </button>
                <button
                  onClick={() => handleBackupSave(selectedGame)}
                  disabled={loading}
                  style={{
                    flex: 1,
                    padding: '10px',
                    background: 'linear-gradient(135deg, #0095ff, #0070cc)',
                    border: 'none',
                    borderRadius: '6px',
                    color: '#fff',
                    fontSize: '14px',
                    fontWeight: 600,
                    cursor: loading ? 'not-allowed' : 'pointer',
                    opacity: loading ? 0.5 : 1,
                  }}
                >
                  {loading ? 'Backing up...' : 'Backup'}
                </button>
              </div>
            </div>
          </div>
        )}

        {/* Backups List */}
        {status.enabled && backups.length > 0 && (
          <div style={{
            marginTop: '16px',
            padding: '16px',
            background: 'rgba(255,255,255,0.03)',
            border: '1px solid rgba(255,255,255,0.08)',
            borderRadius: '8px',
          }}>
            <div style={{ fontSize: '14px', fontWeight: 600, color: '#ccc', marginBottom: '12px' }}>
              Available Backups ({backups.length})
            </div>

            <div style={{ maxHeight: '200px', overflowY: 'auto' }}>
              {backups.map((backup) => (
                <div key={backup.id} style={{
                  padding: '8px 12px',
                  background: 'rgba(255,255,255,0.05)',
                  border: '1px solid rgba(255,255,255,0.1)',
                  borderRadius: '6px',
                  marginBottom: '8px',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '12px',
                }}>
                  <div style={{ flex: 1 }}>
                    <div style={{ fontSize: '13px', fontWeight: 600, color: '#fff' }}>
                      {backup.name}
                      <span style={{
                        marginLeft: '8px',
                        padding: '2px 6px',
                        background: backup.location === 'google_drive'
                          ? 'rgba(0,149,255,0.2)'
                          : 'rgba(255,255,255,0.1)',
                        border: backup.location === 'google_drive'
                          ? '1px solid rgba(0,149,255,0.3)'
                          : '1px solid rgba(255,255,255,0.2)',
                        borderRadius: '3px',
                        fontSize: '10px',
                        color: backup.location === 'google_drive' ? '#0095ff' : '#999',
                      }}>
                        {backup.location === 'google_drive' ? 'Cloud' : 'Local'}
                      </span>
                    </div>
                    <div style={{ fontSize: '11px', color: '#999', marginTop: '2px' }}>
                      Size: {(backup.size / 1024 / 1024).toFixed(2)} MB
                      {backup.createdAt && ` • ${backup.createdAt}`}
                    </div>
                  </div>

                  <button
                    onClick={() => {
                      setSelectedBackup(backup)
                      setShowRestoreDialog(true)
                    }}
                    style={{
                      padding: '6px 12px',
                      background: 'rgba(0,200,83,0.15)',
                      border: '1px solid rgba(0,200,83,0.3)',
                      borderRadius: '4px',
                      color: '#00c853',
                      fontSize: '11px',
                      fontWeight: 600,
                      cursor: 'pointer',
                    }}
                  >
                    Restore
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Restore Confirmation Dialog */}
        {showRestoreDialog && selectedBackup && (
          <div style={{
            position: 'fixed',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            background: 'rgba(0,0,0,0.7)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 9999,
          }}>
            <div style={{
              background: '#1a1a1a',
              border: '1px solid rgba(0,200,83,0.3)',
              borderRadius: '12px',
              padding: '24px',
              maxWidth: '400px',
              width: '90%',
            }}>
              <h3 style={{ color: '#00c853', marginBottom: '16px' }}>⚠️ Restore Backup</h3>
              <p style={{ color: '#ccc', fontSize: '14px', marginBottom: '20px' }}>
                This will restore save data from <strong>{selectedBackup.name}</strong>.
                <br /><br />
                <strong>⚠️ Current save data will be overwritten!</strong>
              </p>
              <div style={{ display: 'flex', gap: '12px' }}>
                <button
                  onClick={() => setShowRestoreDialog(false)}
                  style={{
                    flex: 1,
                    padding: '10px',
                    background: 'rgba(255,255,255,0.1)',
                    border: '1px solid rgba(255,255,255,0.2)',
                    borderRadius: '6px',
                    color: '#fff',
                    fontSize: '14px',
                    cursor: 'pointer',
                  }}
                >
                  Cancel
                </button>
                <button
                  onClick={() => handleRestoreBackup(selectedBackup)}
                  disabled={loading}
                  style={{
                    flex: 1,
                    padding: '10px',
                    background: 'linear-gradient(135deg, #00c853, #00a843)',
                    border: 'none',
                    borderRadius: '6px',
                    color: '#fff',
                    fontSize: '14px',
                    fontWeight: 600,
                    cursor: loading ? 'not-allowed' : 'pointer',
                    opacity: loading ? 0.5 : 1,
                  }}
                >
                  {loading ? 'Restoring...' : 'Restore'}
                </button>
              </div>
            </div>
          </div>
        )}

        {/* Reset Confirmation Dialog */}
        {showResetDialog && selectedGame && (
          <div style={{
            position: 'fixed',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            background: 'rgba(0,0,0,0.7)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 9999,
          }}>
            <div style={{
              background: '#1a1a1a',
              border: '1px solid rgba(255,0,0,0.3)',
              borderRadius: '12px',
              padding: '24px',
              maxWidth: '400px',
              width: '90%',
            }}>
              <h3 style={{ color: '#ff4444', marginBottom: '16px' }}>⚠️ Reset Game Progress</h3>
              <p style={{ color: '#ccc', fontSize: '14px', marginBottom: '20px' }}>
                This will <strong>DELETE ALL SAVE DATA</strong> for <strong>{selectedGame.gameName}</strong>.
                A backup will be created before deletion.
                <br /><br />
                This action cannot be undone!
              </p>
              <div style={{ display: 'flex', gap: '12px' }}>
                <button
                  onClick={() => setShowResetDialog(false)}
                  style={{
                    flex: 1,
                    padding: '10px',
                    background: 'rgba(255,255,255,0.1)',
                    border: '1px solid rgba(255,255,255,0.2)',
                    borderRadius: '6px',
                    color: '#fff',
                    fontSize: '14px',
                    cursor: 'pointer',
                  }}
                >
                  Cancel
                </button>
                <button
                  onClick={() => handleResetGame(selectedGame)}
                  disabled={loading}
                  style={{
                    flex: 1,
                    padding: '10px',
                    background: 'linear-gradient(135deg, #ff4444, #cc0000)',
                    border: 'none',
                    borderRadius: '6px',
                    color: '#fff',
                    fontSize: '14px',
                    fontWeight: 600,
                    cursor: loading ? 'not-allowed' : 'pointer',
                    opacity: loading ? 0.5 : 1,
                  }}
                >
                  {loading ? 'Resetting...' : 'Reset Progress'}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
