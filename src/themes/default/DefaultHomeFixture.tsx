import { useMemo, useState } from 'react'
import heroDefault from '../../assets/hero.png'
import heroBigPicture from '../../assets/showcase/big-picture-1920.webp'
import heroSocial from '../../assets/showcase/social-1920.webp'
import { fallbackCatalog } from '../../lib/installPaths'
import type { GameCatalog, GameInstallState, GameRuntimeState } from '../../types'
import DefaultHomeView from './DefaultHomeView'

const baseGame = fallbackCatalog.games[0]

const fixtureCatalog: GameCatalog = {
  defaultLocale: 'en-US',
  games: [
    { ...baseGame, id: 'frontiers', appid: 2840770, title: 'Frontiers of Pandora', subtitle: 'The western frontier is calling', developer: 'Massive Entertainment', heroAssetId: 'fixture-home-frontiers', gridAssetId: 'fixture-home-frontiers' },
    { ...baseGame, id: 'first-light', appid: 2195780, title: '007 First Light', subtitle: 'Earn the number', developer: 'IO Interactive', heroAssetId: 'fixture-home-first-light', gridAssetId: 'fixture-home-first-light' },
    { ...baseGame, id: 'black-flag', appid: 242050, title: 'Black Flag Resynced', subtitle: 'Return to the golden age of piracy', developer: 'Ubisoft', heroAssetId: 'fixture-home-black-flag', gridAssetId: 'fixture-home-black-flag' },
    { ...baseGame, id: 'geometry-dash', appid: 322170, title: 'Geometry Dash', subtitle: 'Jump to the rhythm', developer: 'RobTop Games', heroAssetId: 'fixture-home-geometry', gridAssetId: 'fixture-home-geometry' },
  ],
}

const fixtureAssets: Record<string, string> = {
  'fixture-home-frontiers': heroDefault,
  'fixture-home-first-light': heroBigPicture,
  'fixture-home-black-flag': heroSocial,
  'fixture-home-geometry': heroDefault,
}

const fixtureInstallStates: Record<string, GameInstallState> = Object.fromEntries(
  fixtureCatalog.games.slice(0, 3).map((game) => [game.id, {
    gameId: game.id,
    installed: true,
    currentVersion: '2.0.50',
    installPath: `E:\\Fixture\\${game.id}`,
    launchExecutable: game.install.launchExecutable,
  }]),
)

const fixtureRuntimeStates: GameRuntimeState[] = fixtureCatalog.games.slice(0, 3).map((game, index) => ({
  gameId: game.id,
  running: index === 1,
  pid: index === 1 ? 7007 : null,
  totalPlaytimeSeconds: (index + 2) * 7_200,
  currentSessionStartedAt: index === 1 ? new Date(Date.now() - 18 * 60_000).toISOString() : null,
  lastPlayedAt: new Date(Date.now() - index * 3_600_000).toISOString(),
  launchCount: 4 + index,
}))

export default function DefaultHomeFixture() {
  const installStates = useMemo(() => fixtureInstallStates, [])
  const runtimeStates = useMemo(() => fixtureRuntimeStates, [])
  const [lastAction, setLastAction] = useState('none')

  return (
    <div style={{ width: '100vw', height: '100svh', overflow: 'hidden' }}>
      <DefaultHomeView
        catalog={fixtureCatalog}
        installStates={installStates}
        runtimeStates={runtimeStates}
        assets={fixtureAssets}
        job={null}
        launcherUpdate={{ version: '2.0.51', notes: 'Cinematic runtime update', publishedAt: new Date().toISOString() }}
        launcherUpdateProgress={null}
        preferences={{ showContinuePlaying: true, showRecentGames: true, showActiveTasks: true, showDiscordCard: true, showDonateCard: true, carouselAutoplay: true }}
        reducedMotion={false}
        onRequestAsset={() => undefined}
        onOpenGame={() => undefined}
        onPlayGame={() => undefined}
        onOpenTab={() => undefined}
        onOpenDiscord={() => setLastAction('discord')}
        onOpenDonate={() => setLastAction('donate')}
        displayName="Fixture User"
        online
      />
      <output data-fixture-action className="default-home-fixture-action">{lastAction}</output>
    </div>
  )
}
