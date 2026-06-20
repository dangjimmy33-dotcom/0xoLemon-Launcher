import { useState, useEffect } from 'react'
import { doc, onSnapshot, addDoc, collection, serverTimestamp } from 'firebase/firestore'
import { db } from './firebase'
import { StoreLibraryView } from './components/library'
import type { GameCatalog, GameInstallState, GameRuntimeState } from './types'
import { MotionConfig } from 'framer-motion'

export function PhoneApp() {
  const [discordId, setDiscordId] = useState(() => localStorage.getItem('phone_discord_id') || '')
  const [catalog, setCatalog] = useState<GameCatalog | null>(null)
  const [installStates, setInstallStates] = useState<Record<string, GameInstallState>>({})
  const [runtimeStates, setRuntimeStates] = useState<GameRuntimeState[]>([])
  const [assets, setAssets] = useState<Record<string, string>>({})
  const [selectedGameId, setSelectedGameId] = useState<string | null>(null)

  useEffect(() => {
    if (!discordId) return
    const unsub = onSnapshot(doc(db, 'users', discordId, 'pc_state', 'current'), (snapshot) => {
      if (snapshot.exists()) {
        const data = snapshot.data()
        if (data.catalog) setCatalog(data.catalog)
        if (data.installStates) setInstallStates(data.installStates)
        if (data.runtimeStates) {
          // Gracefully handle older PC app versions that uploaded a Record
          if (Array.isArray(data.runtimeStates)) {
            setRuntimeStates(data.runtimeStates)
          } else {
            setRuntimeStates(Object.values(data.runtimeStates))
          }
        }
        if (data.assets) setAssets(data.assets)
      }
    })
    return unsub
  }, [discordId])

  const sendCommand = async (action: 'install' | 'launch', gameId: string) => {
    if (!discordId) return
    await addDoc(collection(db, 'users', discordId, 'commands'), {
      action,
      game_id: gameId,
      timestamp: serverTimestamp()
    })
    alert(`Command '${action}' sent to your PC!`)
  }

  if (!discordId) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100vh', background: '#09090b', color: 'white', fontFamily: 'sans-serif' }}>
        <h2 style={{ marginBottom: 10 }}>Remote Control Setup</h2>
        <p style={{ opacity: 0.7, marginBottom: 30, textAlign: 'center', padding: '0 20px' }}>
          Enter your Discord ID to connect to your PC Launcher.
        </p>
        <input 
          type="text" 
          placeholder="e.g. 174002283670"
          value={discordId}
          onChange={(e) => setDiscordId(e.target.value)}
          style={{ padding: '12px', borderRadius: '6px', border: '1px solid #333', background: '#111', color: 'white', width: '80%', maxWidth: '300px', fontSize: '16px' }}
        />
        <button 
          onClick={() => {
            if (discordId) {
              localStorage.setItem('phone_discord_id', discordId)
              setDiscordId(discordId) // trigger re-render
            }
          }}
          style={{ marginTop: '20px', padding: '12px 24px', background: '#5865F2', border: 'none', borderRadius: '6px', color: 'white', cursor: 'pointer', fontWeight: 'bold', fontSize: '16px' }}
        >Connect to PC</button>
      </div>
    )
  }

  if (!catalog) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', height: '100vh', background: '#09090b', color: 'white', fontFamily: 'sans-serif' }}>
        <div style={{ width: 40, height: 40, borderRadius: '50%', border: '4px solid #333', borderTopColor: '#5865F2', animation: 'spin 1s linear infinite', marginBottom: 20 }} />
        <p>Waiting for PC to sync catalog...</p>
        <button 
          onClick={() => {
            localStorage.removeItem('phone_discord_id')
            setDiscordId('')
          }}
          style={{ position: 'absolute', top: 20, right: 20, padding: '8px 16px', background: 'rgba(255,255,255,0.1)', border: 'none', color: 'white', borderRadius: 6, fontWeight: '500' }}
        >Disconnect</button>
        <style>{`@keyframes spin { 100% { transform: rotate(360deg); } }`}</style>
      </div>
    )
  }

  const selectedGame = selectedGameId ? catalog.games.find(g => g.id === selectedGameId) : catalog.games[0]

  return (
    <MotionConfig transition={{ type: 'spring', bounce: 0, duration: 0.4 }}>
      <div className="app-container" data-os="windows">
        <StoreLibraryView 
          viewMode="library"
          catalog={catalog}
          catalogLoadState="ready"
          onRetryCatalog={() => {}}
          selectedGame={selectedGame!}
          selectedGameId={selectedGame?.id || null}
          onSelectGame={setSelectedGameId}
          onRequestAsset={() => {}}
          detail={null}
          assets={assets} // Pass fetched assets from Firestore to display images correctly
          selectedVersion="unknown"
          selectedCurrentVersion="unknown"
          selectedVersionInfo={undefined}
          selectedInstallState={selectedGame ? installStates[selectedGame.id] : undefined}
          verifyStatus={null}
          updateReady={false}
          showVersionAction={false}
          canUpdate={false}
          updateSize={0}
          installSize={0}
          temporarySpace={0}
          isJobRunning={false}
          isGameRunning={selectedGame ? !!runtimeStates.find(s => s.gameId === selectedGame.id)?.running : false}
          onPrimaryAction={() => selectedGame && sendCommand('install', selectedGame.id)}
          onPlay={() => selectedGame && sendCommand('launch', selectedGame.id)}
          onVerify={() => {}}
          onUninstall={() => {}}
          onOpenInstallOptions={() => selectedGame && sendCommand('install', selectedGame.id)}
          onOpenStore={() => {}}
          cloudSaveStatus={null}
          cloudSaveBusy={false}
          cloudLaunchBlocked={false}
          onToggleCloudSave={() => {}}
          onAddCloudSaveFolder={() => {}}
          onSyncCloudSave={() => {}}
          onResolveCloudConflict={() => {}}
          onRestoreCloudSnapshot={() => {}}
          onLaunchWithoutCloudSync={() => {}}
          onConnectGoogleDrive={() => {}}
          onDisconnectGoogleDrive={() => {}}
          onBackupGoogleDrive={() => {}}
          onRestoreMissingSaveFiles={() => {}}
        />
        <button 
          onClick={() => {
            localStorage.removeItem('phone_discord_id')
            setDiscordId('')
          }}
          style={{ position: 'fixed', bottom: 20, right: 20, padding: '10px 20px', background: '#e11d48', border: 'none', color: 'white', borderRadius: 8, zIndex: 100000, fontWeight: 'bold', boxShadow: '0 4px 12px rgba(0,0,0,0.5)' }}
        >Disconnect</button>
      </div>
    </MotionConfig>
  )
}
