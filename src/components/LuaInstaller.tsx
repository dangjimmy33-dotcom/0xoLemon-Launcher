import { useState, useCallback, useEffect, useMemo } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { stat } from '@tauri-apps/plugin-fs'
import { Upload, RefreshCw, CheckCircle, XCircle, AlertCircle, Search } from 'lucide-react'
import { LuaGameItem } from './LuaGameItem'
import './LuaInstaller.css'

type InstallStatus = 'idle' | 'processing' | 'success' | 'error'

interface DroppedFile {
  name: string
  path: string
  size: number
}

export function LuaInstaller() {
  const [files, setFiles] = useState<(File | DroppedFile)[]>([])
  const [status, setStatus] = useState<InstallStatus>('idle')
  const [message, setMessage] = useState('')
  const [isDragOver, setIsDragOver] = useState(false)
  const [installedLuas, setInstalledLuas] = useState<string[]>([])
  const [luaSearch, setLuaSearch] = useState('')
  const [resolvedNames, setResolvedNames] = useState<Record<string, string>>({})
  const [visibleCount, setVisibleCount] = useState(20)

  const filteredLuas = useMemo(() => {
    const q = luaSearch.trim().toLowerCase()
    if (!q) return installedLuas
    return installedLuas.filter(id => {
      if (id.toLowerCase().includes(q)) return true
      const name = resolvedNames[id]
      if (name && name.toLowerCase().includes(q)) return true
      return false
    })
  }, [installedLuas, luaSearch, resolvedNames])

  // Reset visible count when search or list changes
  useEffect(() => {
    setVisibleCount(20)
  }, [filteredLuas])

  const handleScroll = useCallback((e: React.UIEvent<HTMLDivElement>) => {
    const target = e.currentTarget
    if (target.scrollHeight - target.scrollTop - target.clientHeight < 100) {
      setVisibleCount(prev => Math.min(prev + 20, filteredLuas.length))
    }
  }, [filteredLuas.length])

  const fetchInstalled = useCallback(async () => {
    try {
      const luas = await invoke<string[]>('list_installed_luas')
      setInstalledLuas(luas)
    } catch (err) {
      console.error('Failed to fetch installed luas:', err)
    }
  }, [])

  useEffect(() => {
    fetchInstalled()
  }, [fetchInstalled])

  // Listen for Tauri file drop events
  useEffect(() => {
    if (typeof window === 'undefined') return

    let unlisten: (() => void) | undefined

    const setupListener = async () => {
      try {
        const appWindow = getCurrentWindow()
        unlisten = await appWindow.onDragDropEvent(async (event) => {
          if (event.payload.type === 'over') {
            setIsDragOver(true)
          } else if (event.payload.type === 'drop') {
            setIsDragOver(false)
            const paths = event.payload.paths
              if (paths && paths.length > 0) {
                const validExtensions = ['.zip', '.rar', '.7z', '.lua', '.manifest']
                const validPaths = paths.filter(p => validExtensions.some(ext => p.toLowerCase().endsWith(ext)))
                
                if (validPaths.length > 0) {
                  const newFiles: DroppedFile[] = []
                  for (const filePath of validPaths) {
                    const fileName = filePath.split(/[\\/]/).pop() || 'unknown'
                    try {
                      const fileInfo = await stat(filePath)
                      newFiles.push({ name: fileName, path: filePath, size: fileInfo.size })
                    } catch (err) {
                      console.error('[LuaInstaller] stat FAILED:', filePath, '→', err)
                    }
                  }
                  
                  if (newFiles.length > 0) {
                    setFiles(newFiles)
                    setStatus('idle')
                    setMessage('')
                  } else {
                    setStatus('error')
                    setMessage('Failed to read dropped files.')
                  }
                } else {
                  setStatus('error')
                  setMessage('Please drop valid files (.zip, .rar, .7z, .lua, .manifest)')
                }
              }
          } else if (event.payload.type === 'leave') {
            setIsDragOver(false)
          }
        })
      } catch (error) {
        console.error('Failed to setup drag drop listener:', error)
      }
    }

    void setupListener()

    return () => {
      if (unlisten) unlisten()
    }
  }, [])

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragOver(false)

    if (!e.dataTransfer.files || e.dataTransfer.files.length === 0) {
      setStatus('error')
      setMessage('No files detected')
      return
    }

    const validExtensions = ['.zip', '.rar', '.7z', '.lua', '.manifest']
    const validFiles = Array.from(e.dataTransfer.files).filter(f => 
      validExtensions.some(ext => f.name.toLowerCase().endsWith(ext))
    )

    if (validFiles.length > 0) {
      setFiles(validFiles)
      setStatus('idle')
      setMessage('')
    } else {
      setStatus('error')
      setMessage('Please drop valid files (.zip, .rar, .7z, .lua, .manifest)')
    }
  }, [])

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragOver(true)
  }, [])

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragOver(false)
  }, [])

  const handleFileSelect = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    if (!e.target.files || e.target.files.length === 0) return

    const validExtensions = ['.zip', '.rar', '.7z', '.lua', '.manifest']
    const validFiles = Array.from(e.target.files).filter(f => 
      validExtensions.some(ext => f.name.toLowerCase().endsWith(ext))
    )

    if (validFiles.length > 0) {
      setFiles(validFiles)
      setStatus('idle')
      setMessage('')
    } else {
      setStatus('error')
      setMessage('Please select valid files (.zip, .rar, .7z, .lua, .manifest)')
    }
  }, [])

  const handleInstall = useCallback(async () => {
    if (files.length === 0) return

    setStatus('processing')
    setMessage('Installing files...')

    try {
      for (const f of files) {
        let fileData: string

        if ('path' in f && typeof f.path === 'string') {
          fileData = await invoke<string>('read_file_base64', { filepath: f.path })
        } else {
          const reader = new FileReader()
          fileData = await new Promise<string>((resolve, reject) => {
            reader.onload = () => resolve((reader.result as string).split(',')[1])
            reader.onerror = reject
            reader.readAsDataURL(f as File)
          })
        }

        // We still use install_lua_from_zip for backwards compatibility with the rust command name, 
        // but now the Rust command will handle different extensions.
        // To tell Rust what the file extension is, we can pass it via appid for now, e.g. "123456.manifest",
        // but wait, install_lua_from_zip in Rust currently expects appid as String.
        // Actually, we can just send the full filename as appid and parse it in Rust!
        await invoke('install_lua_from_zip', {
          appid: f.name, // Send FULL filename to backend so it knows the extension
          zipDataBase64: fileData
        })
      }

      setStatus('success')
      setMessage('All files installed successfully! Restart Steam to apply changes.')
      setFiles([])
      fetchInstalled()
    } catch (error) {
      setStatus('error')
      setMessage(`Installation failed: ${error}`)
    }
  }, [files, fetchInstalled])

  const handleRestartSteam = useCallback(async () => {
    try {
      await invoke('restart_steam')
      setMessage('Steam is restarting...')
    } catch (error) {
      setMessage(`Failed to restart Steam: ${error}`)
    }
  }, [])

  return (
    <div className="lua-installer-container">
      <div className="lua-installer-content">
        <header className="lua-installer-header">
          <h1>Lua Installer</h1>
          <p>Install .lua files, .manifest files, or .zip archives to Steam</p>
          <button
            type="button"
            className="restart-steam-btn"
            onClick={handleRestartSteam}
            title="Restart Steam"
          >
            <RefreshCw size={16} />
            Restart Steam
          </button>
        </header>

        <div
          className={`drop-zone ${isDragOver ? 'drag-over' : ''} ${files.length > 0 ? 'has-file' : ''}`}
          onDrop={handleDrop}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
        >
          {files.length > 0 ? (
            <div className="file-info">
              <Upload size={48} />
              <span className="file-name">{files.length} file(s) selected</span>
              <button className="clear-btn" onClick={() => setFiles([])}>Clear</button>
            </div>
          ) : (
            <div className="drop-placeholder">
              <Upload size={48} />
              <p>Drop .zip, .rar, .7z, or .lua file here</p>
              <input
                type="file"
                accept=".zip,.rar,.7z,.lua,.manifest"
                onChange={handleFileSelect}
                multiple
                style={{ display: 'none' }}
                id="file-input"
              />
              <label htmlFor="file-input" className="browse-btn">
                Browse Files
              </label>
            </div>
          )}
        </div>

        {message && (
          <div className={`status-message status-${status}`}>
            {status === 'processing' && <AlertCircle size={20} />}
            {status === 'success' && <CheckCircle size={20} />}
            {status === 'error' && <XCircle size={20} />}
            <span>{message}</span>
          </div>
        )}

        <div className="actions">
          <button
            type="button"
            className="install-btn primary-control"
            onClick={handleInstall}
            disabled={files.length === 0 || status === 'processing'}
          >
            {status === 'processing' ? 'Installing...' : 'Install'}
          </button>
          {files.length > 0 && (
            <button
              type="button"
              className="clear-btn"
              onClick={() => {
                setFiles([])
                setStatus('idle')
                setMessage('')
              }}
              disabled={status === 'processing'}
            >
              Clear
            </button>
          )}
        </div>

        <div className="instructions">
          <h3>Instructions:</h3>
          <ol>
            <li>Download the {'{appid}'}.zip file for your game</li>
            <li>Drag and drop the zip file into the area above</li>
            <li>Click "Install" to setup the Lua script</li>
            <li>Click "Restart Steam" to apply changes</li>
            <li>The game will appear in your Steam library</li>
          </ol>
        </div>
      </div>

      {/* Installed Luas panel */}
      <div style={{ marginTop: '24px', background: 'rgba(255,255,255,0.02)', borderRadius: '12px', border: '1px solid rgba(255,255,255,0.06)', overflow: 'hidden', width: '100%', maxWidth: '700px' }}>
        {/* Header */}
        <div style={{ padding: '14px 16px 10px', borderBottom: '1px solid rgba(255,255,255,0.05)', display: 'flex', alignItems: 'center', gap: '12px' }}>
          <div style={{ flex: 1 }}>
            <span style={{ fontSize: '14px', fontWeight: 700, color: '#ccc' }}>Installed Luas</span>
            <span style={{ marginLeft: '8px', fontSize: '12px', color: '#555', fontVariantNumeric: 'tabular-nums' }}>
              {installedLuas.length > 0 ? `${filteredLuas.length}${luaSearch ? `/${installedLuas.length}` : ''} games` : ''}
            </span>
          </div>
          {/* Search */}
          {installedLuas.length > 5 && (
            <div style={{ position: 'relative', display: 'flex', alignItems: 'center' }}>
              <Search size={13} style={{ position: 'absolute', left: '9px', color: '#555', pointerEvents: 'none' }} />
              <input
                type="text"
                placeholder="Search by name or appid…"
                value={luaSearch}
                onChange={e => setLuaSearch(e.target.value)}
                style={{
                  background: 'rgba(255,255,255,0.05)',
                  border: '1px solid rgba(255,255,255,0.08)',
                  borderRadius: '6px',
                  color: '#ccc',
                  fontSize: '12px',
                  padding: '5px 10px 5px 28px',
                  outline: 'none',
                  width: '180px',
                }}
              />
            </div>
          )}
          <button
            onClick={fetchInstalled}
            title="Refresh list"
            style={{ background: 'transparent', border: 'none', color: '#555', cursor: 'pointer', padding: '4px', display: 'flex', alignItems: 'center', borderRadius: '4px' }}
            onMouseEnter={e => (e.currentTarget.style.color = '#aaa')}
            onMouseLeave={e => (e.currentTarget.style.color = '#555')}
          >
            <RefreshCw size={14} />
          </button>
        </div>

        {/* List */}
        <div 
          style={{ maxHeight: '420px', overflowY: 'auto', padding: '10px 12px', display: 'flex', flexDirection: 'column', gap: '6px' }}
          onScroll={handleScroll}
        >
          {installedLuas.length === 0 ? (
            <p style={{ color: '#555', fontSize: '13px', margin: '8px 0', textAlign: 'center' }}>No Lua manifests installed.</p>
          ) : filteredLuas.length === 0 ? (
            <p style={{ color: '#555', fontSize: '13px', margin: '8px 0', textAlign: 'center' }}>No results for "{luaSearch}"</p>
          ) : (
            filteredLuas.slice(0, visibleCount).map(appid => (
              <LuaGameItem 
                key={appid} 
                appid={appid} 
                onRemoved={fetchInstalled}
                onNameLoaded={(name: string) => {
                  setResolvedNames(prev => prev[appid] === name ? prev : { ...prev, [appid]: name })
                }}
              />
            ))
          )}
        </div>
      </div>
    </div>
  )
}
