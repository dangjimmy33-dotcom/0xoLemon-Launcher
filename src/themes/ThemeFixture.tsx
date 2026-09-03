import { useEffect, useMemo, useState } from 'react'
import { Download, Library, Play, RefreshCcw, Search, Settings2 } from 'lucide-react'
import { applyLauncherTheme } from '../lib/theme'
import { loadLauncherPreferences, type LauncherPreferences } from '../lib/preferences'
import type { UiThemeId } from '../lib/uiThemes'
import type { LauncherSettings, TabId, XmclInstanceGroup } from '../types'
import { ThemeShellHost } from './ThemeShellHost'
import { ThemeSettingsHost } from './ThemeSettingsHost'
import { GameToolsView } from '../components/GameToolsHub'
import type { ThemeInstanceViewModel } from './contracts'
import { SteamLibraryHome } from './steam/SteamLibraryHome'
import { SteamLibraryDetail } from './steam/SteamLibraryDetail'
import { SteamStoreHome } from './steam/SteamStoreHome'
import { fallbackCatalog } from '../lib/installPaths'
import hero007 from '../assets/hero-007.png'
import heroDefault from '../assets/hero.png'
import './theme-fixture.css'

const FIXTURE_INSTANCES: readonly ThemeInstanceViewModel[] = [
  { gameId: 'frontiers', title: 'Frontiers of Pandora', developer: 'Massive Entertainment', description: 'Explore the living western frontier of Pandora.', heroUrl: heroDefault, gridUrl: heroDefault, installed: true, favorite: true, playing: false },
  { gameId: 'first-light', title: '007 First Light', developer: 'IO Interactive', description: 'Follow Bond at the beginning of his career.', heroUrl: hero007, gridUrl: hero007, installed: true, favorite: false, playing: true },
  { gameId: 'black-flag', title: 'Black Flag Resynced', developer: 'Ubisoft Singapore', description: 'Return to the golden age of piracy.', heroUrl: hero007, gridUrl: hero007, installed: false, favorite: true, playing: false },
  { gameId: 'geometry-dash', title: 'Geometry Dash', developer: 'RobTop Games', description: 'Jump and fly through rhythm-based levels.', heroUrl: heroDefault, gridUrl: heroDefault, installed: true, favorite: false, playing: false },
]

const FIXTURE_GROUPS: readonly XmclInstanceGroup[] = [
  { id: 'all-instances', name: 'All instances', gameIds: FIXTURE_INSTANCES.map((item) => item.gameId), collapsed: false },
  { id: 'favorites', name: 'Favorites', gameIds: FIXTURE_INSTANCES.filter((item) => item.favorite).map((item) => item.gameId), collapsed: false },
]

const FIXTURE_LAUNCHER_SETTINGS: LauncherSettings = {
  defaultLibrary: 'E:\\0xoLemon store',
  downloadWorkers: 8,
  downloadRetries: 5,
  packRangeMb: 16,
  keepChunkCache: true,
  notificationsEnabled: true,
  autoVerifyAfterInstall: false,
  downloadProfile: 'balanced',
  downloadQueueMb: 128,
  directToStaging: true,
  cloudSaveRoot: '',
  gameUpdateMode: 'manual',
  gameUpdateScheduleStart: '02:00',
  gameUpdateScheduleEnd: '06:00',
  depotHfRepoId: '',
  gameTurbo: 'ask',
}

const fixtureBaseGame = fallbackCatalog.games[0]
const FIXTURE_GAMES = [
  { ...fixtureBaseGame, id: 'frontiers', title: 'Frontiers of Pandora', developer: 'Massive Entertainment', gridAssetId: 'fixture-grid-frontiers', heroAssetId: 'fixture-hero-frontiers' },
  { ...fixtureBaseGame, id: 'first-light', title: '007 First Light', developer: 'IO Interactive', gridAssetId: 'fixture-grid-first-light', heroAssetId: 'fixture-hero-first-light' },
  { ...fixtureBaseGame, id: 'black-flag', title: 'Black Flag Resynced', developer: 'Ubisoft Singapore', gridAssetId: 'fixture-grid-black-flag', heroAssetId: 'fixture-hero-black-flag' },
  { ...fixtureBaseGame, id: 'geometry-dash', title: 'Geometry Dash', developer: 'RobTop Games', gridAssetId: 'fixture-grid-geometry', heroAssetId: 'fixture-hero-geometry' },
]

const FIXTURE_LIBRARY_ASSETS = {
  'fixture-grid-frontiers': heroDefault,
  'fixture-hero-frontiers': heroDefault,
  'fixture-grid-first-light': hero007,
  'fixture-hero-first-light': hero007,
  'fixture-grid-black-flag': hero007,
  'fixture-hero-black-flag': hero007,
  'fixture-grid-geometry': heroDefault,
  'fixture-hero-geometry': heroDefault,
}

const FIXTURE_GAME_TOOLS_ITEMS = FIXTURE_INSTANCES.map((instance) => ({
  gameId: instance.gameId,
  appId: Number(FIXTURE_GAMES.find((game) => game.id === instance.gameId)?.appid ?? 0) || null,
  title: instance.title,
  subtitle: instance.description ?? '',
  imageUrl: instance.gridUrl ?? null,
  installed: instance.installed,
}))

const FIXTURE_DETAIL = {
  gameId: 'fixture-game',
  locale: 'en-US',
  title: 'Fixture game',
  shortDescription: 'A focused library fixture used to validate Steam activity and sidebar geometry.',
  detailedDescription: '',
  developers: ['0xoLemon Studio'],
  publishers: ['0xoLemon'],
  releaseDate: 'Available now',
  genres: ['Adventure'],
  categories: ['Single-player', 'Cloud saves'],
  ratings: [],
  media: [],
  achievements: [],
  sounds: [],
  install: fixtureBaseGame.install,
  cloudSave: fixtureBaseGame.cloudSave,
  steamRuntime: fixtureBaseGame.steamRuntime,
  achievementsEnabled: fixtureBaseGame.achievementsEnabled,
  saveProviders: fixtureBaseGame.saveProviders,
  descriptionImages: [],
  versions: fixtureBaseGame.availableVersions,
  metadataSource: 'fixture',
}

function FixtureSteamRail({ selectedGameId, onSelectGame }: {
  selectedGameId: string | null
  onSelectGame: (gameId: string | null) => void
}) {
  return (
    <aside className={selectedGameId ? 'steam-library-detail-rail' : 'steam-library-rail'} aria-label="Library games">
      <button type="button" className={`steam-library-home-button${selectedGameId ? '' : ' is-active'}`} onClick={() => onSelectGame(null)}>
        <Library size={15} /><span>Home</span><small>{FIXTURE_GAMES.length}</small>
      </button>
      <label className="steam-library-search"><Search size={14} /><input placeholder="Search library" /><kbd>Ctrl K</kbd></label>
      <div className="steam-library-rail-heading"><span>Games</span><small>{FIXTURE_GAMES.length}</small></div>
      <div className="steam-library-game-list">
        {FIXTURE_GAMES.map((game) => (
          <button key={game.id} type="button" className={`steam-library-game-row${game.id === selectedGameId ? ' is-active' : ''}`} onClick={() => onSelectGame(game.id)}>
            <span className="steam-library-game-icon"><Library size={13} /></span>
            <span className="steam-library-game-name">{game.title}</span>
            <span className="steam-library-installed-dot" />
          </button>
        ))}
      </div>
    </aside>
  )
}

function FixtureLibrary({
  theme,
  selectedGameId,
  onSelectGame,
}: {
  theme: Extract<UiThemeId, 'lightning' | 'steam' | 'xmcl'>
  selectedGameId: string | null
  onSelectGame: (gameId: string | null) => void
}) {
  const selected = FIXTURE_INSTANCES.find((item) => item.gameId === selectedGameId)
  const selectedGame = FIXTURE_GAMES.find((item) => item.id === selectedGameId)

  if (theme === 'steam' && !selected) {
    return (
      <section className="library-browse-view has-steam-library-rail steam-library-home-view theme-fixture-steam-library">
        <FixtureSteamRail selectedGameId={null} onSelectGame={onSelectGame} />
        <SteamLibraryHome
          games={FIXTURE_GAMES}
          assets={FIXTURE_LIBRARY_ASSETS}
          installStates={{
            frontiers: { installed: true },
            'first-light': { installed: true },
            'geometry-dash': { installed: true },
          } as never}
          favoriteGameIds={new Set(['frontiers', 'black-flag'])}
          onSelectGame={onSelectGame}
          onCustomizeShelves={() => undefined}
          onRequestAsset={() => undefined}
        />
      </section>
    )
  }

  if (theme === 'steam' && selected && selectedGame) {
    const hero = FIXTURE_LIBRARY_ASSETS[selectedGame.heroAssetId as keyof typeof FIXTURE_LIBRARY_ASSETS]
    const cover = FIXTURE_LIBRARY_ASSETS[selectedGame.gridAssetId as keyof typeof FIXTURE_LIBRARY_ASSETS]
    return (
      <section className="game-detail-view has-steam-library-rail steam-library-game-detail steam-library-dedicated-detail theme-fixture-steam-detail">
        <FixtureSteamRail selectedGameId={selectedGame.id} onSelectGame={onSelectGame} />
        <SteamLibraryDetail
          game={selectedGame}
          detail={{ ...FIXTURE_DETAIL, gameId: selectedGame.id, title: selectedGame.title, developers: [selectedGame.developer], publishers: [selectedGame.publisher] }}
          assets={FIXTURE_LIBRARY_ASSETS}
          installState={{ gameId: selectedGame.id, installed: selected.installed, currentVersion: '2.0.50', installPath: 'E:\\Fixture', launchExecutable: 'fixture.exe' }}
          displayedVersion="2.0.50"
          heroUrl={hero}
          coverUrl={cover}
          downloadSize={selectedGame.availableVersions[0]?.sizeBytes ?? 0}
          installed={selected.installed}
          updateReady={false}
          installing={false}
          playing={selected.playing}
          verifying={false}
          installBlocked={false}
          favorite={selected.favorite}
          showVersionAction
          verifyStatus={null}
          onInstall={() => undefined}
          onPlay={() => undefined}
          onStop={() => undefined}
          onUpdate={() => undefined}
          onVersions={() => undefined}
          onVerify={() => undefined}
          onBrowse={() => undefined}
          onUninstall={() => undefined}
          onToggleFavorite={() => undefined}
          onOpenStore={() => undefined}
        />
      </section>
    )
  }

  return (
    <div className="theme-fixture-page">
      <header className="theme-fixture-heading">
        <div><span>LIBRARY</span><h1>{selected?.title || 'Game collection'}</h1></div>
        <label><Search size={15} /><input aria-label="Search fixture games" placeholder="Search games" /></label>
      </header>
      {selected ? (
        <section className="theme-fixture-detail">
          <div className="theme-fixture-hero"><span>FEATURED INSTANCE</span><strong>{selected.title}</strong></div>
          <div className="theme-fixture-actions">
            <button type="button"><Play size={15} fill="currentColor" /> Play</button>
            <button type="button"><Settings2 size={15} /> Manage</button>
          </div>
          <div className="theme-fixture-stats"><span>Ready to play</span><span>Version 2.0.50</span><span>Cloud synchronized</span></div>
        </section>
      ) : (
        <section className="theme-fixture-grid">
          {FIXTURE_INSTANCES.map((instance, index) => (
            <article key={instance.gameId}>
              <div className={`theme-fixture-art art-${index + 1}`}><span>{instance.title.slice(0, 1)}</span></div>
              <strong>{instance.title}</strong>
              <small>{instance.installed ? 'Ready to play' : 'Available in Store'}</small>
            </article>
          ))}
        </section>
      )}
    </div>
  )
}

export default function ThemeFixture({ theme }: { theme: Extract<UiThemeId, 'lightning' | 'steam' | 'xmcl'> }) {
  const [activeTab, setActiveTab] = useState<TabId>(() => {
    if (theme === 'lightning') {
      const parameters = new URLSearchParams(window.location.search)
      const section = parameters.get('section')
      if (section && ['store', 'tools', 'bypass', 'onlineFix'].includes(section)) {
        window.localStorage.setItem('0xolemon.game-tools.section', section)
      }
      if (parameters.get('tab') === 'Tools') return 'Tools'
      return 'Home'
    }
    return 'Library'
  })
  const [selectedGameId, setSelectedGameId] = useState<string | null>(null)
  const [history, setHistory] = useState<TabId[]>([])
  const [fixturePreferences, setFixturePreferences] = useState(loadLauncherPreferences)
  const [fixtureLauncherSettings, setFixtureLauncherSettings] = useState(FIXTURE_LAUNCHER_SETTINGS)
  const selected = useMemo(
    () => FIXTURE_INSTANCES.find((item) => item.gameId === selectedGameId) ?? null,
    [selectedGameId],
  )

  useEffect(() => {
    const preferences = loadLauncherPreferences()
    applyLauncherTheme({ ...preferences, uiTheme: theme, themeAccentMode: 'native' })
    return () => applyLauncherTheme({ ...preferences, uiTheme: 'default' })
  }, [theme])

  const navigate = (tab: TabId) => {
    setHistory((current) => [...current, activeTab].slice(-12))
    setActiveTab(tab)
  }

  return (
    <ThemeShellHost
      theme={theme}
      activeTab={activeTab}
      onNavigate={navigate}
      onBack={() => {
        const previous = history.at(-1)
        if (!previous) return
        setHistory((current) => current.slice(0, -1))
        setActiveTab(previous)
      }}
      onForward={() => undefined}
      canGoBack={history.length > 0}
      canGoForward={false}
      serviceStatus="Online"
      updateCount={2}
      downloadCount={1}
      luaModeEnabled
      discordAuthorized
      displayName="Fixture User"
      selectedGameTitle={selected?.title ?? null}
      selectedGameId={selectedGameId}
      instances={FIXTURE_INSTANCES}
      instanceGroups={FIXTURE_GROUPS}
      onSelectGame={(gameId) => { setSelectedGameId(gameId); navigate('Library') }}
      onOpenSelfProfile={() => navigate('Social')}
      isSidebarCollapsed={false}
      onToggleSidebar={() => undefined}
    >
      <div className="workspace-corner-clip">
        <section className="workspace premium-workspace theme-fixture-workspace">
          {activeTab === 'Library' ? (
            <FixtureLibrary theme={theme} selectedGameId={selectedGameId} onSelectGame={setSelectedGameId} />
          ) : activeTab === 'Tools' && theme === 'lightning' ? (
            <GameToolsView
              steamInstalledAppIds={[2840770, 2195780, 242050]}
              libraryItems={FIXTURE_GAME_TOOLS_ITEMS}
              onNavigate={navigate}
              onReloadLibrary={() => undefined}
            />
          ) : activeTab === 'Store' && theme === 'steam' ? (
            <SteamStoreHome
              games={FIXTURE_GAMES}
              assets={FIXTURE_LIBRARY_ASSETS}
              ownedGameIds={new Set(['frontiers', 'first-light'])}
              wishlistGameIds={new Set(['black-flag'])}
              onSelectGame={(gameId) => { setSelectedGameId(gameId); navigate('Store') }}
              onRequestAsset={() => undefined}
            />
          ) : activeTab === 'Settings' ? (
            <div className="tab-content settings-tab-content">
              <ThemeSettingsHost
                theme={theme}
                preferences={fixturePreferences}
                launcherSettings={fixtureLauncherSettings}
                onChange={<K extends keyof LauncherPreferences>(key: K, value: LauncherPreferences[K]) => {
                  setFixturePreferences((current) => ({ ...current, [key]: value }))
                }}
                onLauncherSettingChange={<K extends keyof LauncherSettings>(key: K, value: LauncherSettings[K]) => {
                  setFixtureLauncherSettings((current) => ({ ...current, [key]: value }))
                }}
                onChooseLibrary={() => undefined}
                onOpenLibrary={() => undefined}
                onOpenCache={() => undefined}
                onOpenCloudRedirect={() => navigate('CloudRedirect')}
                onChooseCloudRoot={() => undefined}
                onOpenCloudRoot={() => undefined}
                onCheckForUpdates={() => undefined}
                onLuaGameModeChange={() => undefined}
                steamEnvironment={null}
                steamStatus="Steam fixture ready"
                onRefreshSteam={() => undefined}
                onOpenSteam={() => undefined}
                onRestartSteam={() => undefined}
                onOpenBigPicture={() => undefined}
                onReset={() => undefined}
                onResetOnboarding={() => undefined}
                onOpenHelpCenter={() => undefined}
                onManageNotifications={() => undefined}
                sessionUiTheme={theme}
                onClose={() => navigate('Home')}
                appVersion="2.0.50"
                updateStatus="Up to date"
              />
            </div>
          ) : (
            <div className="theme-fixture-page">
              <header className="theme-fixture-heading"><div><span>0XOLEMON</span><h1>{activeTab}</h1></div></header>
              <section className="theme-fixture-utility">
                <Download size={24} /><strong>{activeTab} workspace</strong>
                <span>Shared launcher actions remain available through the active theme renderer.</span>
                <button type="button" onClick={() => navigate('Library')}><RefreshCcw size={14} /> Return to Library</button>
              </section>
            </div>
          )}
        </section>
      </div>
    </ThemeShellHost>
  )
}
