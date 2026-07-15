import { useState, useCallback, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { stat } from '@tauri-apps/plugin-fs'
import { Upload, RefreshCw, CheckCircle, XCircle, AlertCircle } from 'lucide-react'
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

      <div className="lua-installed-list" style={{ marginTop: '30px', background: 'rgba(255,255,255,0.03)', padding: '20px', borderRadius: '12px', border: '1px solid rgba(255,255,255,0.05)' }}>
        <h3 style={{ margin: '0 0 15px 0', fontSize: '15px', fontWeight: 600, color: '#e0e0e0' }}>Installed Luas</h3>
        {installedLuas.length === 0 ? (
          <p style={{ color: '#888', fontSize: '13px', margin: 0 }}>No Lua manifests installed in Steam config.</p>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
            {installedLuas.map(appid => (
              <div key={appid} style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', background: 'rgba(255,255,255,0.05)', padding: '8px 12px', borderRadius: '6px' }}>
                <span style={{ fontFamily: 'monospace', fontSize: '13px', color: '#ccc' }}>{appid}.lua</span>
                <button 
                  onClick={async () => {
                    if (confirm(`Are you sure you want to remove lua ${appid}?`)) {
                      try {
                        await invoke('remove_from_steam', { appid: parseInt(appid) })
                        fetchInstalled()
                      } catch(e) {
                        alert('Failed to remove: ' + e)
                      }
                    }
                  }}
                  style={{ background: 'rgba(255,50,50,0.15)', color: '#ff6b6b', border: 'none', padding: '4px 10px', borderRadius: '4px', cursor: 'pointer', fontSize: '12px', fontWeight: 500 }}
                >
                  Delete
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
