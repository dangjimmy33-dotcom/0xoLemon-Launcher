import { memo, useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Trash2 } from 'lucide-react'
import { fetchSteamGameInfo, getCachedSteamGameInfo, type SteamGameInfo } from '../lib/steamGameInfo'

type LuaGameItemProps = {
  appid: string
  onRemoved: () => void
  onNameLoaded?: (appid: string, name: string) => void
  scrollRoot?: { current: HTMLDivElement | null }
}

export const LuaGameItem = memo(function LuaGameItem({
  appid,
  onRemoved,
  onNameLoaded,
  scrollRoot,
}: LuaGameItemProps) {
  const [info, setInfo] = useState<SteamGameInfo | null>(getCachedSteamGameInfo(appid) ?? null)
  const [shouldLoad, setShouldLoad] = useState(() => getCachedSteamGameInfo(appid) !== undefined)
  const [isLoading, setIsLoading] = useState(() => getCachedSteamGameInfo(appid) === undefined)
  const rowRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (shouldLoad) return
    const row = rowRef.current
    if (!row || typeof IntersectionObserver === 'undefined') {
      setShouldLoad(true)
      return
    }

    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting) {
        setShouldLoad(true)
        observer.disconnect()
      }
    }, {
      root: scrollRoot?.current ?? null,
      rootMargin: '96px 0px',
      threshold: 0.01,
    })
    observer.observe(row)
    return () => observer.disconnect()
  }, [scrollRoot, shouldLoad])

  useEffect(() => {
    if (!shouldLoad) return

    const cached = getCachedSteamGameInfo(appid)
    if (cached) {
      setInfo(cached)
      setIsLoading(false)
      onNameLoaded?.(appid, cached.name)
      return
    }

    let mounted = true
    fetchSteamGameInfo(appid).then((result) => {
      if (!mounted) return
      setInfo(result)
      setIsLoading(false)
      if (result?.name) onNameLoaded?.(appid, result.name)
    })
    return () => {
      mounted = false
    }
  }, [appid, onNameLoaded, shouldLoad])

  const handleRemove = async () => {
    const displayName = info?.name || appid
    if (!confirm('Are you sure you want to remove lua for ' + displayName + '?')) return

    try {
      await invoke('remove_from_steam', { appid: parseInt(appid) })
      onRemoved()
    } catch (error) {
      alert('Failed to remove: ' + error)
    }
  }

  const imageUrl = shouldLoad
    ? info?.header_image ||
      'https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/' + appid + '/header.jpg'
    : ''

  return (
    <div
      ref={rowRef}
      className="lua-game-row"
      style={{
        display: 'flex',
        alignItems: 'center',
        background: 'rgba(255,255,255,0.04)',
        borderRadius: '8px',
        border: '1px solid rgba(255,255,255,0.06)',
        overflow: 'hidden',
        transition: 'background 0.15s',
        height: '52px',
        flexShrink: 0,
      }}
      onMouseEnter={(event) => { event.currentTarget.style.background = 'rgba(255,255,255,0.07)' }}
      onMouseLeave={(event) => { event.currentTarget.style.background = 'rgba(255,255,255,0.04)' }}
    >
      <div style={{ width: '92px', height: '52px', flexShrink: 0, background: 'rgba(0,0,0,0.3)', overflow: 'hidden' }}>
        {isLoading ? (
          <div style={{ width: '100%', height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <div style={{ width: '18px', height: '18px', border: '2px solid rgba(255,255,255,0.1)', borderTopColor: 'rgba(255,255,255,0.5)', borderRadius: '50%', animation: 'spin 0.8s linear infinite' }} />
          </div>
        ) : imageUrl ? (
          <img
            src={imageUrl}
            alt={info?.name || appid}
            loading="lazy"
            decoding="async"
            style={{ width: '100%', height: '100%', objectFit: 'cover', display: 'block' }}
            onError={(event) => {
              const image = event.currentTarget
              if (!image.src.includes('capsule_231x87')) {
                image.src = 'https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/' + appid + '/capsule_231x87.jpg'
              } else {
                image.style.opacity = '0'
              }
            }}
          />
        ) : null}
      </div>

      <div style={{ flex: 1, padding: '0 12px', minWidth: 0 }}>
        <div style={{ fontSize: '13px', fontWeight: 600, color: '#ddd', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
          {isLoading ? <span style={{ color: '#555' }}>Loading...</span> : (info?.name || <span style={{ color: '#666', fontStyle: 'italic' }}>AppID {appid}</span>)}
        </div>
        <div style={{ fontSize: '11px', color: '#555', fontFamily: 'monospace', marginTop: '2px' }}>
          {appid}.lua
        </div>
      </div>

      <button
        onClick={handleRemove}
        title="Remove this lua"
        style={{
          background: 'transparent', color: '#ef4444', border: 'none',
          padding: '0 14px', height: '100%', cursor: 'pointer',
          display: 'flex', alignItems: 'center', flexShrink: 0,
          transition: 'background 0.15s',
        }}
        onMouseEnter={(event) => { event.currentTarget.style.background = 'rgba(239,68,68,0.12)' }}
        onMouseLeave={(event) => { event.currentTarget.style.background = 'transparent' }}
      >
        <Trash2 size={15} />
      </button>
    </div>
  )
})
