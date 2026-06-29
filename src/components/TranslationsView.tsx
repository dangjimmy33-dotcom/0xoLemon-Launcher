import { useEffect, useState } from 'react'
import { Languages, Download, Trash2, ChevronLeft } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import { ScopedTabEmptyState } from './layout'

interface TranslationInfo {
  file_name: string
  path: string
  size: number
}

interface TranslationsViewProps {
  gameId?: string
  gameTitle?: string
  gameBanner?: string
  onVerify?: () => void
  onBack?: () => void
}

function formatTranslationName(fileName: string, gameTitle?: string): string {
  let name = fileName.replace(/\.7z$/i, '')
  if (gameTitle) {
    const titleRegex = new RegExp(gameTitle.replace(/[^a-zA-Z0-9]/g, '.*'), 'i')
    name = name.replace(titleRegex, '')
  }
  name = name.replace(/viethoa/i, '')
  name = name.replace(/[\.\-_]+/g, ' ').trim()
  
  if (!name) return 'Default Translation'
  if (name.toLowerCase() === 'full') return 'Full Translation'
  
  // capitalize first letter
  return 'Patch: ' + name.charAt(0).toUpperCase() + name.slice(1)
}

export function TranslationsView({ gameId, gameTitle, gameBanner, onVerify, onBack }: TranslationsViewProps) {
  const [translations, setTranslations] = useState<TranslationInfo[]>([])
  const [loading, setLoading] = useState(false)
  const [installing, setInstalling] = useState<string | null>(null)
  const [uninstalling, setUninstalling] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!gameId) return
    setLoading(true)
    setError(null)
    invoke<TranslationInfo[]>('get_available_translations', { gameId })
      .then(setTranslations)
      .catch(e => setError(String(e)))
      .finally(() => setLoading(false))
  }, [gameId])

  if (!gameId) {
    return (
      <ScopedTabEmptyState
        icon={<Languages size={34} />}
        title="No game selected"
        body="Choose a game in Library to view and manage its translations."
      />
    )
  }

  const handleInstall = async (path: string) => {
    try {
      setInstalling(path)
      setError(null)
      await invoke('install_translation', { gameId, translationPath: path })
      // Consider showing a success toast here
    } catch (e) {
      setError(`Failed to install patch: ${e}`)
    } finally {
      setInstalling(null)
    }
  }

  const handleUninstall = async () => {
    try {
      setUninstalling(true)
      setError(null)
      await invoke('uninstall_translation', { gameId })
      if (onVerify) {
        onVerify()
      }
    } catch (e) {
      setError(`Failed to uninstall patch: ${e}`)
    } finally {
      setUninstalling(false)
    }
  }

  return (
    <section className="game-detail-main translations-view">
      {onBack && (
        <button className="back-to-library" type="button" onClick={onBack} style={{ margin: '20px 40px 0' }}>
          <ChevronLeft size={16} />
          Back
        </button>
      )}

      {gameBanner ? (
        <div className="detail-hero" style={{ margin: '20px 40px', minHeight: '300px' }}>
          <img src={gameBanner} alt={gameTitle} loading="eager" />
          <div className="detail-hero-shade" />
          <div className="detail-copy">
            <span className="storage-pill" style={{ display: 'inline-flex', alignItems: 'center', gap: '6px' }}>
              <Languages size={14} />
              Translation Manager
            </span>
            <h1>{gameTitle}</h1>
            <p>Manage and install community translation patches for this game.</p>
          </div>
        </div>
      ) : (
        <header className="cr-header" style={{ marginBottom: '20px', padding: '20px 40px' }}>
          <div className="cr-header-title" style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
            <Languages size={24} />
            <h2 style={{ margin: 0, fontSize: '24px' }}>Translations: {gameTitle}</h2>
          </div>
          <p style={{ color: 'var(--muted)', marginTop: '8px' }}>
            Manage and install translation patches for this game.
          </p>
        </header>
      )}
      
      <div className="cr-body" style={{ padding: '0 40px', maxWidth: '900px' }}>
        {error && (
          <div style={{ padding: '12px', background: 'var(--bg-error, #ffebee)', color: 'var(--text-error, #c62828)', borderRadius: '6px', marginBottom: '20px' }}>
            {error}
          </div>
        )}

        <div style={{ display: 'flex', gap: '10px', marginBottom: '24px', marginTop: '20px' }}>
          <button 
            className="cr-button secondary" 
            onClick={handleUninstall}
            disabled={uninstalling || installing !== null}
            style={{ display: 'flex', alignItems: 'center', gap: '8px' }}
          >
            <Trash2 size={16} />
            {uninstalling ? 'Uninstalling...' : 'Uninstall Translation'}
          </button>
        </div>

        {loading ? (
          <div style={{ padding: '40px', textAlign: 'center', color: 'var(--muted)', background: 'var(--bg-card)', borderRadius: '8px', border: '1px solid var(--border)' }}>
            Loading available translations...
          </div>
        ) : translations.length === 0 ? (
          <div style={{ padding: '40px 20px', textAlign: 'center', background: 'var(--bg-card)', borderRadius: '8px', border: '1px solid var(--border)' }}>
            <p style={{ margin: 0, color: 'var(--muted)' }}>No translations found for this game.</p>
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
            {translations.map((t, index) => {
              const displayName = formatTranslationName(t.file_name, gameTitle);
              return (
                <div key={t.path} style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '16px 20px', background: 'var(--bg-card)', borderRadius: '8px', border: '1px solid var(--border)', transition: 'background 0.2s' }}>
                  <div>
                    <h4 style={{ margin: '0 0 6px 0', fontSize: '16px' }}>{displayName}</h4>
                    <span style={{ fontSize: '13px', color: 'var(--muted)', display: 'flex', gap: '10px' }}>
                      <span>Version {index + 1}</span>
                      <span>•</span>
                      <span>{(t.size / 1024 / 1024).toFixed(2)} MB</span>
                    </span>
                  </div>
                  <button
                    className="cr-button primary"
                    onClick={() => handleInstall(t.path)}
                    disabled={installing !== null || uninstalling}
                    style={{ display: 'flex', alignItems: 'center', gap: '8px', padding: '8px 16px' }}
                  >
                    {installing === t.path ? (
                      <>Installing...</>
                    ) : (
                      <>
                        <Download size={16} />
                        Install
                      </>
                    )}
                  </button>
                </div>
              )
            })}
          </div>
        )}
      </div>
    </section>
  )
}
