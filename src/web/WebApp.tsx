import {
  ArrowDownToLine,
  ArrowRight,
  Check,
  ChevronRight,
  CircleAlert,
  Cloud,
  Download,
  ExternalLink,
  Gamepad2,
  Gauge,
  HardDrive,
  History,
  Laptop,
  Library,
  LoaderCircle,
  LockKeyhole,
  LogOut,
  Menu,
  MonitorCheck,
  Play,
  RefreshCw,
  Search,
  ShieldCheck,
  Smartphone,
  Sparkles,
  UserRound,
  Wifi,
  WifiOff,
  X,
} from 'lucide-react'
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from 'react'
import social1280 from '../assets/showcase/social-1280.webp'
import social1920 from '../assets/showcase/social-1920.webp'
import delivery1280 from '../assets/showcase/smart-delivery-1280.webp'
import delivery1920 from '../assets/showcase/smart-delivery-1920.webp'
import personalization1280 from '../assets/showcase/personalization-1280.webp'
import personalization1920 from '../assets/showcase/personalization-1920.webp'
import bigPicture1280 from '../assets/showcase/big-picture-1280.webp'
import bigPicture1920 from '../assets/showcase/big-picture-1920.webp'
import { createRequestId, formatBytes, webApi, WebApiError } from './api'
import type {
  LauncherDevice,
  RemoteJob,
  WebCatalog,
  WebCatalogGame,
  WebSession,
} from './types'

type PublicRoute =
  | '/'
  | '/features'
  | '/download'
  | '/changelog'
  | '/help'
  | '/status'
  | '/terms'
  | '/privacy'
  | '/community-guidelines'
  | '/security'
  | '/third-party-notices'
  | '/auth/error'
  | '/app'

type DashboardSection = 'catalog' | 'library' | 'devices' | 'jobs' | 'profile'

const PUBLIC_ROUTES = new Set<PublicRoute>([
  '/', '/features', '/download', '/changelog', '/help', '/status', '/terms', '/privacy',
  '/community-guidelines', '/security', '/third-party-notices', '/app',
  '/auth/error',
])

const PRODUCT_LINKS: Array<{ path: PublicRoute; label: string }> = [
  { path: '/features', label: 'Features' },
  { path: '/download', label: 'Download' },
  { path: '/changelog', label: 'Changelog' },
  { path: '/help', label: 'Help' },
  { path: '/status', label: 'Status' },
]

function normalizeRoute(pathname: string): PublicRoute {
  const clean = pathname.length > 1 ? pathname.replace(/\/+$/, '') : pathname
  return PUBLIC_ROUTES.has(clean as PublicRoute) ? clean as PublicRoute : '/'
}

function navigate(path: PublicRoute) {
  if (window.location.pathname === path) return
  window.history.pushState({}, '', path)
  window.dispatchEvent(new PopStateEvent('popstate'))
  window.scrollTo({ top: 0, behavior: 'auto' })
}

function RouteLink({ path, children, className = '' }: { path: PublicRoute; children: ReactNode; className?: string }) {
  return (
    <a
      href={path}
      className={className}
      onClick={(event) => {
        if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
        event.preventDefault()
        navigate(path)
      }}
    >
      {children}
    </a>
  )
}

function PublicHeader({ route }: { route: PublicRoute }) {
  const [open, setOpen] = useState(false)
  return (
    <header className="web-header">
      <RouteLink path="/" className="web-brand" aria-label="0xoLemon home">
        <span className="web-brand-mark">0x</span>
        <span>0xoLemon</span>
      </RouteLink>
      <button className="web-nav-toggle" type="button" onClick={() => setOpen((value) => !value)} aria-label="Toggle navigation" aria-expanded={open}>
        {open ? <X /> : <Menu />}
      </button>
      <nav className={open ? 'web-nav is-open' : 'web-nav'} aria-label="Main navigation">
        {PRODUCT_LINKS.map((item) => (
          <RouteLink key={item.path} path={item.path} className={route === item.path ? 'is-active' : ''}>{item.label}</RouteLink>
        ))}
      </nav>
      <RouteLink path="/app" className="web-account-link"><MonitorCheck /> Remote dashboard</RouteLink>
    </header>
  )
}

function PublicFooter() {
  return (
    <footer className="web-footer">
      <div><strong>0xoLemon Launcher</strong><span>Desktop delivery, Steam tools and remote control in one account.</span></div>
      <nav aria-label="Legal links">
        <RouteLink path="/terms">Terms</RouteLink>
        <RouteLink path="/privacy">Privacy</RouteLink>
        <RouteLink path="/community-guidelines">Community</RouteLink>
        <RouteLink path="/security">Security</RouteLink>
        <RouteLink path="/third-party-notices">Third-party notices</RouteLink>
      </nav>
      <small>0xoLemon is not affiliated with Valve Corporation, Discord Inc. or game publishers.</small>
    </footer>
  )
}

function LandingPage() {
  return (
    <>
      <main>
        <section className="web-hero">
          <picture>
            <source media="(max-width: 1280px)" srcSet={social1280} />
            <img src={social1920} alt="0xoLemon Launcher social and library interface" fetchPriority="high" />
          </picture>
          <div className="web-hero-scrim" />
          <div className="web-hero-copy">
            <p className="web-eyebrow">WINDOWS GAME LAUNCHER</p>
            <h1>0xoLemon Launcher</h1>
            <p>Install the exact game version you choose, keep large downloads resumable, and control your own online launcher from the web.</p>
            <div className="web-hero-actions">
              <RouteLink path="/download" className="web-button web-button-primary"><Download /> Download launcher</RouteLink>
              <RouteLink path="/app" className="web-button web-button-quiet"><MonitorCheck /> Open remote dashboard</RouteLink>
            </div>
          </div>
          <div className="web-hero-status"><span /><span>Transactional installs</span><span>Delta-aware updates</span><span>Remote jobs</span></div>
        </section>

        <section className="web-value-band">
          <div><strong>Resume without starting over</strong><span>Verified staging and durable journals keep completed work across restarts.</span></div>
          <div><strong>Choose the target directly</strong><span>Fresh installs use the chosen manifest, not a chain of every previous release.</span></div>
          <div><strong>Your machine stays in control</strong><span>The website dispatches typed requests. The launcher validates and executes locally.</span></div>
        </section>

        <FeatureBand image1280={delivery1280} image1920={delivery1920} eyebrow="SMART DELIVERY" title="Network speed without trading away integrity." body="Parallel transport, bounded queues, per-chunk verification and atomic commits keep large installs responsive while preserving repair, downgrade and patch behavior." icon={<Gauge />} />
        <FeatureBand image1280={bigPicture1280} image1920={bigPicture1920} eyebrow="LIVING ROOM" title="A complete launcher from desktop to Big Picture." body="Use the same catalog, jobs, social identity and game state whether you are at a desk or navigating with a controller." icon={<Gamepad2 />} reverse />
        <FeatureBand image1280={personalization1280} image1920={personalization1920} eyebrow="ONE ACCOUNT" title="Remote requests go only to your verified launcher." body="Discord authentication, device credentials and explicit device selection prevent a download intended for you from being sent to someone else's PC." icon={<ShieldCheck />} />

        <section className="web-final-cta">
          <p className="web-eyebrow">READY WHEN YOUR PC IS</p>
          <h2>Start locally. Continue remotely.</h2>
          <p>Install the launcher, connect Remote Web Access, then send a versioned job to an online device.</p>
          <div><RouteLink path="/download" className="web-button web-button-primary"><ArrowDownToLine /> Get 0xoLemon</RouteLink><RouteLink path="/features" className="web-button web-button-quiet">Explore features <ArrowRight /></RouteLink></div>
        </section>
      </main>
      <PublicFooter />
    </>
  )
}

function FeatureBand({ image1280, image1920, eyebrow, title, body, icon, reverse = false }: {
  image1280: string
  image1920: string
  eyebrow: string
  title: string
  body: string
  icon: ReactNode
  reverse?: boolean
}) {
  return (
    <section className={reverse ? 'web-feature-band is-reverse' : 'web-feature-band'}>
      <div className="web-feature-media"><picture><source media="(max-width: 1280px)" srcSet={image1280} /><img src={image1920} alt="" loading="lazy" decoding="async" /></picture></div>
      <div className="web-feature-copy"><span className="web-feature-icon">{icon}</span><p className="web-eyebrow">{eyebrow}</p><h2>{title}</h2><p>{body}</p><RouteLink path="/features">See how it works <ChevronRight /></RouteLink></div>
    </section>
  )
}

const DOCUMENTS: Record<Exclude<PublicRoute, '/' | '/app' | '/features' | '/download' | '/auth/error'>, { title: string; lead: string; sections: Array<{ title: string; body: string }> }> = {
  '/changelog': {
    title: 'Changelog', lead: 'Release notes for the launcher, backend and web control surface.',
    sections: [
      { title: 'Remote Web preview', body: 'Discord web sessions, verified launcher devices, typed remote jobs, live progress and legal versioning are available behind canary flags.' },
      { title: 'Transport pipeline v3', body: 'Adaptive Hugging Face transport and separate network, verification and writer stages improve throughput without removing integrity checks.' },
      { title: 'Launcher ecosystem', body: 'Game Tools, Lua source management, CloudRedirect and social presence share one authenticated launcher state.' },
    ],
  },
  '/help': {
    title: 'Help center', lead: 'Resolve common launcher, download, Steam and remote access issues.',
    sections: [
      { title: 'A remote device is offline', body: 'Open 0xoLemon on the target PC, sign into the same Discord account and enable Remote Web Access. Offline devices cannot receive queued jobs.' },
      { title: 'A download is paused or interrupted', body: 'Resume from Downloads. The logical total remains the original job total while already verified bytes are retained.' },
      { title: 'A game is missing after reinstalling the launcher', body: 'Use Locate existing library once. Registered library markers let future installations recover games without hashing the entire drive.' },
    ],
  },
  '/status': {
    title: 'Service status', lead: 'Live service checks are exposed separately from launcher job state.',
    sections: [
      { title: 'Launcher downloads', body: 'Hugging Face remains the source of game payloads. Transport health and retries are shown inside each job.' },
      { title: 'Remote access', body: 'Remote Web is deployed behind feature flags and a canary allowlist until end-to-end verification is complete.' },
      { title: 'Degraded mode', body: 'Local installs, Library and game launch remain available when the website or remote backend is unavailable.' },
    ],
  },
  '/terms': {
    title: 'Terms of use', lead: 'Version 2026-08-25. Owner review is required before production rollout.',
    sections: [
      { title: 'Your account and devices', body: 'You are responsible for activity initiated through your Discord-authenticated session and for securing devices connected to your account.' },
      { title: 'Remote requests', body: 'Remote Web sends a typed request to an online launcher. The launcher may reject a request when the catalog, version, storage or local state does not match.' },
      { title: 'Third-party services', body: 'Availability can depend on Discord, Hugging Face, Steam, Firebase, Render and game publisher infrastructure.' },
    ],
  },
  '/privacy': {
    title: 'Privacy', lead: 'Version 2026-08-25. Remote access is designed to avoid exposing Discord tokens and local paths to the browser.',
    sections: [
      { title: 'Authentication', body: 'Discord authorization is exchanged by the backend. The browser receives an opaque HttpOnly session cookie, not a Discord access or refresh token.' },
      { title: 'Device metadata', body: 'The service stores a hashed account identifier, device label, operating system, launcher version, opaque library IDs, free-space summaries and remote job audit data.' },
      { title: 'Local data', body: 'Windows paths and device credentials remain on the launcher. Device credentials are protected using Windows DPAPI.' },
    ],
  },
  '/community-guidelines': {
    title: 'Community guidelines', lead: 'Keep shared spaces useful, lawful and safe for other members.',
    sections: [
      { title: 'Respect people', body: 'Do not harass, impersonate, threaten or expose personal information about other members.' },
      { title: 'Respect access boundaries', body: 'Do not attempt to use another person’s account, device, library or remote credential.' },
      { title: 'Report problems responsibly', body: 'Security and abuse reports should include reproducible facts without publishing secrets or exploit payloads.' },
    ],
  },
  '/security': {
    title: 'Security', lead: 'Remote control is restricted by identity, device ownership and a narrow command contract.',
    sections: [
      { title: 'No raw command surface', body: 'The website cannot supply a URL, Windows path, executable or shell command. It can only identify an allowed action, game, version, device and registered library.' },
      { title: 'Session protection', body: 'OAuth uses Authorization Code with PKCE. Sessions use HttpOnly cookies, CSRF checks, rotation and server-side revocation.' },
      { title: 'Report a vulnerability', body: 'Provide the affected version, expected boundary and minimal reproduction privately. Never include a live user token or signing key.' },
    ],
  },
  '/third-party-notices': {
    title: 'Third-party notices', lead: '0xoLemon combines open-source libraries and external services under their respective terms.',
    sections: [
      { title: 'Desktop software', body: 'Tauri, Rust crates, React and supporting JavaScript packages retain their upstream licenses and notices.' },
      { title: 'Integrated projects', body: 'Components ported from permitted open-source projects retain attribution in the launcher distribution and source tree.' },
      { title: 'Service marks', body: 'Steam, Discord, Hugging Face and game names belong to their respective owners. References describe interoperability only.' },
    ],
  },
}

function DocumentPage({ route }: { route: keyof typeof DOCUMENTS }) {
  const document = DOCUMENTS[route]
  return <><main className="web-document"><p className="web-eyebrow">0XOLEMON</p><h1>{document.title}</h1><p className="web-document-lead">{document.lead}</p><div className="web-document-sections">{document.sections.map((section) => <section key={section.title}><h2>{section.title}</h2><p>{section.body}</p></section>)}</div></main><PublicFooter /></>
}

function FeaturesPage() {
  return <><main className="web-page"><header className="web-page-hero"><p className="web-eyebrow">DESKTOP + WEB</p><h1>One launcher, without one fragile workflow.</h1><p>Each surface is purpose-built, while game state and verified jobs remain consistent.</p></header><div className="web-feature-grid"><FeatureCell icon={<HardDrive />} title="Version-aware delivery" body="Install, update or downgrade directly to an exact manifest with resumable verified staging." /><FeatureCell icon={<Cloud />} title="Cloud and local recovery" body="CloudRedirect and library discovery preserve game state across launcher reinstalls and device changes." /><FeatureCell icon={<Smartphone />} title="Remote Web" body="Dispatch typed jobs only to an online launcher authenticated to the same Discord account." /><FeatureCell icon={<Sparkles />} title="Adaptive experiences" body="Desktop, controller, Steam-inspired, XMCL and Lightning renderers share the same underlying actions." /></div></main><PublicFooter /></>
}

function FeatureCell({ icon, title, body }: { icon: ReactNode; title: string; body: string }) {
  return <section><span>{icon}</span><h2>{title}</h2><p>{body}</p></section>
}

function DownloadPage() {
  const downloadUrl = import.meta.env.VITE_LAUNCHER_DOWNLOAD_URL || '#download-unavailable'
  return <><main className="web-download-page"><div><p className="web-eyebrow">WINDOWS 10 / 11</p><h1>Download 0xoLemon Launcher</h1><p>Install the signed desktop client before using Remote Web. The launcher validates game versions, libraries and available storage locally.</p><a className="web-button web-button-primary" href={downloadUrl}><Download /> Download latest release</a><small>Remote Web cannot install games while the launcher is offline.</small></div><picture><source media="(max-width: 1280px)" srcSet={delivery1280} /><img src={delivery1920} alt="0xoLemon download manager" /></picture></main><PublicFooter /></>
}

function LegalGate({ session, onAccepted }: { session: WebSession; onAccepted: () => void }) {
  const [locale, setLocale] = useState<'en' | 'vi'>('en')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const legal = session.legal
  if (!legal) return null

  const submit = async () => {
    setBusy(true)
    setError('')
    try {
      await webApi.acceptLegal({ termsVersion: legal.termsVersion, privacyVersion: legal.privacyVersion, locale })
      onAccepted()
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'LEGAL_ACCEPTANCE_FAILED')
    } finally {
      setBusy(false)
    }
  }

  return <main className="web-auth-state"><div className="web-auth-symbol"><ShieldCheck /></div><p className="web-eyebrow">TERMS UPDATE</p><h1>Review before using Remote Web</h1><p>Remote jobs can start downloads on your online PC. Accept the current Terms and Privacy versions for this account before continuing.</p><div className="web-legal-links"><RouteLink path="/terms">Terms {legal.termsVersion}</RouteLink><RouteLink path="/privacy">Privacy {legal.privacyVersion}</RouteLink></div><label>Document language<select value={locale} onChange={(event) => setLocale(event.target.value as 'en' | 'vi')}><option value="en">English</option><option value="vi">Tiếng Việt</option></select></label>{error && <p className="web-inline-error"><CircleAlert /> {error}</p>}<button className="web-button web-button-primary" type="button" onClick={submit} disabled={busy}>{busy ? <LoaderCircle className="is-spinning" /> : <Check />} Accept and continue</button></main>
}

function RemoteDashboard({ session, refreshSession }: { session: WebSession; refreshSession: () => void }) {
  const [section, setSection] = useState<DashboardSection>('catalog')
  const [catalog, setCatalog] = useState<WebCatalog>({ defaultLocale: 'en-US', games: [] })
  const [devices, setDevices] = useState<LauncherDevice[]>([])
  const [jobs, setJobs] = useState<RemoteJob[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [wizardGame, setWizardGame] = useState<WebCatalogGame | null>(null)
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(0)
  const pageSize = 18

  const loadOperationalState = useCallback(async (quiet = false) => {
    if (!quiet) setLoading(true)
    try {
      const [nextCatalog, nextDevices, nextJobs] = await Promise.all([webApi.catalog(), webApi.devices(), webApi.jobs()])
      setCatalog(nextCatalog)
      setDevices(nextDevices)
      setJobs(nextJobs)
      setError('')
    } catch (caught) {
      if (caught instanceof WebApiError && caught.status === 401) {
        refreshSession()
        return
      }
      setError(caught instanceof Error ? caught.message : 'REMOTE_STATE_FAILED')
    } finally {
      if (!quiet) setLoading(false)
    }
  }, [refreshSession])

  useEffect(() => {
    const timer = window.setTimeout(() => void loadOperationalState(), 0)
    return () => window.clearTimeout(timer)
  }, [loadOperationalState])

  useEffect(() => {
    const source = new EventSource(`${webApi.apiRoot}/remote-events`, { withCredentials: true })
    const updateJob = (event: MessageEvent<string>) => {
      const incoming = JSON.parse(event.data) as RemoteJob
      setJobs((current) => [incoming, ...current.filter((job) => job.id !== incoming.id)])
    }
    const refreshDevices = () => void webApi.devices().then(setDevices).catch(() => undefined)
    source.addEventListener('job.updated', updateJob as EventListener)
    source.addEventListener('device.online', refreshDevices)
    source.addEventListener('device.offline', refreshDevices)
    source.addEventListener('device.updated', refreshDevices)
    // Native EventSource reconnect preserves Last-Event-ID. The polling loop
    // below remains the fallback while Render is restarting or SSE is blocked.
    return () => source.close()
  }, [])

  useEffect(() => {
    const timer = window.setInterval(() => {
      if (document.visibilityState === 'visible') void loadOperationalState(true)
    }, 15_000)
    return () => window.clearInterval(timer)
  }, [loadOperationalState])

  const filteredGames = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    if (!normalized) return catalog.games
    return catalog.games.filter((game) => `${game.title} ${game.id} ${game.developer || ''}`.toLocaleLowerCase().includes(normalized))
  }, [catalog.games, query])
  const pageCount = Math.max(1, Math.ceil(filteredGames.length / pageSize))
  const visiblePage = Math.min(page, pageCount - 1)
  const pageGames = filteredGames.slice(visiblePage * pageSize, (visiblePage + 1) * pageSize)

  const logout = async () => {
    await webApi.logout().catch(() => undefined)
    refreshSession()
  }

  return (
    <main className="remote-shell">
      <header className="remote-header">
        <RouteLink path="/" className="web-brand"><span className="web-brand-mark">0x</span><span>0xoLemon</span></RouteLink>
        <div className="remote-account"><img src={session.user?.avatarUrl || '/favicon.svg'} alt="" /><span><strong>{session.user?.displayName || session.user?.username}</strong><small>Discord verified</small></span><button type="button" onClick={logout} title="Sign out"><LogOut /></button></div>
      </header>
      <div className="remote-layout">
        <nav className="remote-nav" aria-label="Remote dashboard">
          <DashboardNavButton active={section === 'catalog'} onClick={() => setSection('catalog')} icon={<Search />} label="Catalog" />
          <DashboardNavButton active={section === 'library'} onClick={() => setSection('library')} icon={<Library />} label="Library Web" />
          <DashboardNavButton active={section === 'devices'} onClick={() => setSection('devices')} icon={<Laptop />} label="Devices" count={devices.filter((device) => device.online).length} />
          <DashboardNavButton active={section === 'jobs'} onClick={() => setSection('jobs')} icon={<History />} label="Remote jobs" count={jobs.filter((job) => !['completed', 'failed', 'canceled'].includes(job.state)).length} />
          <DashboardNavButton active={section === 'profile'} onClick={() => setSection('profile')} icon={<UserRound />} label="Profile" />
          <div className="remote-nav-status"><span className={devices.some((device) => device.online) ? 'is-online' : ''} />{devices.some((device) => device.online) ? 'Launcher online' : 'No launcher online'}</div>
        </nav>
        <section className="remote-workspace">
          <header className="remote-workspace-header"><div><p className="web-eyebrow">REMOTE WEB</p><h1>{section === 'catalog' ? 'Catalog' : section === 'library' ? 'Library Web' : section === 'devices' ? 'Devices' : section === 'jobs' ? 'Remote jobs' : 'Profile'}</h1></div><button type="button" className="web-icon-button" onClick={() => void loadOperationalState()} title="Refresh"><RefreshCw className={loading ? 'is-spinning' : ''} /></button></header>
          {error && <div className="web-banner-error"><CircleAlert /><span>{error}</span><button type="button" onClick={() => void loadOperationalState()}>Retry</button></div>}
          {loading ? <LoadingRows /> : section === 'catalog' ? (
            <CatalogSection games={pageGames} total={filteredGames.length} query={query} onQuery={(value) => { setQuery(value); setPage(0) }} onInstall={setWizardGame} page={visiblePage} pageCount={pageCount} onPage={setPage} />
          ) : section === 'library' ? (
            <LibraryWebSection games={catalog.games.filter((game) => devices.some((device) => device.installedGameIds.includes(game.id)))} devices={devices} onOpen={(game) => setWizardGame(game)} />
          ) : section === 'devices' ? (
            <DevicesSection devices={devices} onRevoke={async (id) => { await webApi.revokeDevice(id); await loadOperationalState(true) }} />
          ) : section === 'jobs' ? (
            <JobsSection jobs={jobs} games={catalog.games} onCancel={async (id) => { const job = await webApi.cancelJob(id); setJobs((current) => current.map((item) => item.id === job.id ? job : item)) }} />
          ) : (
            <ProfileSection session={session} devices={devices} jobs={jobs} />
          )}
        </section>
      </div>
      {wizardGame && <RemoteJobDialog game={wizardGame} devices={devices} onClose={() => setWizardGame(null)} onCreated={(job) => { setJobs((current) => [job, ...current.filter((item) => item.id !== job.id)]); setWizardGame(null); setSection('jobs') }} />}
    </main>
  )
}

function DashboardNavButton({ active, onClick, icon, label, count }: { active: boolean; onClick: () => void; icon: ReactNode; label: string; count?: number }) {
  return <button type="button" className={active ? 'is-active' : ''} onClick={onClick}>{icon}<span>{label}</span>{typeof count === 'number' && <small>{count}</small>}</button>
}

function LoadingRows() {
  return <div className="remote-loading" aria-label="Loading"><span /><span /><span /><span /></div>
}

function CatalogSection({ games, total, query, onQuery, onInstall, page, pageCount, onPage }: {
  games: WebCatalogGame[]
  total: number
  query: string
  onQuery: (value: string) => void
  onInstall: (game: WebCatalogGame) => void
  page: number
  pageCount: number
  onPage: (value: number) => void
}) {
  return <div className="remote-section"><div className="remote-toolbar"><label><Search /><input value={query} onChange={(event) => onQuery(event.target.value)} placeholder="Search by title or game ID" /></label><span>{total.toLocaleString()} games</span></div><div className="remote-catalog-grid">{games.map((game) => <article key={game.id} className="remote-game"><GameArtwork game={game} /><div><small>{game.developer || '0xoLemon catalog'}</small><h2>{game.title}</h2><p>{game.subtitle || game.latestVersion || 'Version selection available'}</p></div><button type="button" onClick={() => onInstall(game)}><Download /> Install remotely</button></article>)}</div><div className="remote-pagination"><button type="button" disabled={page === 0} onClick={() => onPage(page - 1)}>Previous</button><span>Page {page + 1} / {pageCount}</span><button type="button" disabled={page + 1 >= pageCount} onClick={() => onPage(page + 1)}>Next</button></div></div>
}

function GameArtwork({ game }: { game: WebCatalogGame }) {
  const source = game.gridAssetUrl && /^https:\/\//i.test(game.gridAssetUrl) ? game.gridAssetUrl : ''
  return <div className="remote-game-art">{source ? <img src={source} alt="" loading="lazy" decoding="async" /> : <span>{game.title.slice(0, 1).toUpperCase()}</span>}</div>
}

function LibraryWebSection({ games, devices, onOpen }: { games: WebCatalogGame[]; devices: LauncherDevice[]; onOpen: (game: WebCatalogGame) => void }) {
  return <div className="remote-section"><div className="remote-section-intro"><h2>Installed across your devices</h2><p>Remote Web reports only launcher-confirmed installs. A launch request is sent only to a device that currently reports the game as installed.</p></div>{games.length ? <div className="remote-list">{games.map((game) => <div key={game.id}><GameArtwork game={game} /><span><strong>{game.title}</strong><small>{devices.filter((device) => device.installedGameIds.includes(game.id)).map((device) => device.name).join(', ')}</small></span><button type="button" onClick={() => onOpen(game)}><Play /> Remote action</button></div>)}</div> : <EmptyState icon={<Library />} title="No reported installs" body="Open the desktop launcher and let it connect to Remote Web." />}</div>
}

function DevicesSection({ devices, onRevoke }: { devices: LauncherDevice[]; onRevoke: (id: string) => Promise<void> }) {
  return <div className="remote-section"><div className="remote-section-intro"><h2>Verified launcher devices</h2><p>Only online devices can accept work. Device credentials stay protected by Windows DPAPI.</p></div>{devices.length ? <div className="remote-device-list">{devices.map((device) => <article key={device.id}><header>{device.online ? <Wifi /> : <WifiOff />}<div><h2>{device.name}</h2><p>{device.os} · Launcher {device.launcherVersion || 'unknown'}</p></div><span className={device.online ? 'is-online' : ''}>{device.online ? 'Online' : 'Offline'}</span></header><div className="remote-device-facts"><span><HardDrive /> {device.libraries.length} libraries</span><span><Gamepad2 /> {device.installedGameIds.length} installed</span><span><History /> {device.lastSeen ? new Date(device.lastSeen).toLocaleString() : 'Never seen'}</span></div><div className="remote-library-list">{device.libraries.map((library) => <div key={library.id}><span>{library.label}</span><strong>{formatBytes(library.freeBytes)} free</strong></div>)}</div><button type="button" className="web-danger-button" onClick={() => void onRevoke(device.id)}>Revoke device</button></article>)}</div> : <EmptyState icon={<Laptop />} title="No launcher connected" body="Enable Remote Web Access in the desktop launcher while signed into this Discord account." />}</div>
}

function JobsSection({ jobs, games, onCancel }: { jobs: RemoteJob[]; games: WebCatalogGame[]; onCancel: (id: string) => Promise<void> }) {
  return <div className="remote-section"><div className="remote-section-intro"><h2>Job history</h2><p>Progress is reported by the launcher journal. Refreshing this page does not restart a job.</p></div>{jobs.length ? <div className="remote-jobs">{jobs.map((job) => { const game = games.find((entry) => entry.id === job.gameId); const percent = Math.round((job.progress?.overallProgress || 0) * 100); return <article key={job.id}><header><span className={`job-state job-state-${job.state}`}>{job.state}</span><div><h2>{game?.title || job.gameId}</h2><p>{job.action} · {job.versionId || 'installed version'}</p></div><time>{job.updatedAt ? new Date(job.updatedAt).toLocaleString() : ''}</time></header><div className="remote-job-progress"><span style={{ width: `${percent}%` }} /></div><footer><span>{job.progress?.phase || job.errorCode || 'Waiting for launcher'}</span><strong>{percent}%{job.progress?.speedBytesPerSecond ? ` · ${formatBytes(job.progress.speedBytesPerSecond)}/s` : ''}</strong>{['dispatching', 'accepted', 'running'].includes(job.state) && <button type="button" onClick={() => void onCancel(job.id)}>Cancel</button>}</footer></article> })}</div> : <EmptyState icon={<History />} title="No remote jobs yet" body="Choose a game from Catalog and send it to an online launcher." />}</div>
}

function ProfileSection({ session, devices, jobs }: { session: WebSession; devices: LauncherDevice[]; jobs: RemoteJob[] }) {
  return <div className="remote-section"><div className="remote-profile"><img src={session.user?.avatarUrl || '/favicon.svg'} alt="" /><div><p className="web-eyebrow">DISCORD IDENTITY</p><h2>{session.user?.displayName || session.user?.username}</h2><span>@{session.user?.username}</span></div></div><div className="remote-profile-metrics"><div><Laptop /><strong>{devices.length}</strong><span>Verified devices</span></div><div><Wifi /><strong>{devices.filter((device) => device.online).length}</strong><span>Online now</span></div><div><History /><strong>{jobs.length}</strong><span>Remote jobs</span></div></div><div className="remote-profile-note"><LockKeyhole /><span><strong>Your Discord token is not stored in this browser.</strong> The browser keeps only an opaque HttpOnly session cookie. Local paths and launcher credentials never appear in the web API.</span></div></div>
}

function EmptyState({ icon, title, body }: { icon: ReactNode; title: string; body: string }) {
  return <div className="remote-empty"><span>{icon}</span><h2>{title}</h2><p>{body}</p></div>
}

function RemoteJobDialog({ game, devices, onClose, onCreated }: { game: WebCatalogGame; devices: LauncherDevice[]; onClose: () => void; onCreated: (job: RemoteJob) => void }) {
  const online = useMemo(() => devices.filter((device) => device.online), [devices])
  const versions = game.availableVersions?.length ? game.availableVersions : [{ version: game.latestVersion || 'latest', label: game.latestVersion || 'Latest', latest: true }]
  const [versionId, setVersionId] = useState(versions.find((version) => version.latest)?.version || versions[0]?.version || '')
  const [deviceId, setDeviceId] = useState(online.length === 1 ? online[0].id : '')
  const selectedDevice = online.find((device) => device.id === deviceId)
  const [libraryId, setLibraryId] = useState('')
  const preferredLibrary = selectedDevice?.libraries.find((library) => library.default) || selectedDevice?.libraries[0]
  const effectiveLibraryId = selectedDevice?.libraries.some((library) => library.id === libraryId)
    ? libraryId
    : preferredLibrary?.id || ''
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const dialogRef = useRef<HTMLDialogElement>(null)

  useEffect(() => { dialogRef.current?.showModal() }, [])
  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (!deviceId || !effectiveLibraryId || !versionId) return
    setBusy(true)
    setError('')
    try {
      const job = await webApi.createJob({ action: 'install', gameId: game.id, versionId, deviceId, libraryId: effectiveLibraryId, requestId: createRequestId() })
      onCreated(job)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'REMOTE_JOB_FAILED')
    } finally {
      setBusy(false)
    }
  }

  return <dialog ref={dialogRef} className="remote-job-dialog" onClose={onClose}><form method="dialog" onSubmit={submit}><header><div><p className="web-eyebrow">REMOTE INSTALL</p><h2>{game.title}</h2></div><button type="button" onClick={() => dialogRef.current?.close()} aria-label="Close"><X /></button></header>{online.length === 0 ? <div className="web-banner-error"><WifiOff /><span>No verified launcher is online. Open 0xoLemon on the target PC first.</span></div> : <><label>Version<select value={versionId} onChange={(event) => setVersionId(event.target.value)}>{versions.map((version) => <option key={version.version} value={version.version}>{version.label || version.version}{version.latest ? ' · Latest' : ''}</option>)}</select></label><label>Device<select value={deviceId} onChange={(event) => { setDeviceId(event.target.value); setLibraryId('') }} disabled={online.length === 1}><option value="">Select an online launcher</option>{online.map((device) => <option key={device.id} value={device.id}>{device.name} · {device.os}</option>)}</select>{online.length > 1 && !deviceId && <small>More than one launcher is online. Choose the exact destination.</small>}</label><label>Registered library<select value={effectiveLibraryId} onChange={(event) => setLibraryId(event.target.value)} disabled={!selectedDevice}><option value="">Select a library</option>{selectedDevice?.libraries.map((library) => <option key={library.id} value={library.id}>{library.label} · {formatBytes(library.freeBytes)} free</option>)}</select></label><div className="remote-job-summary"><span><ShieldCheck /> The launcher validates this catalog version</span><span><HardDrive /> No Windows path is sent by this website</span><span><MonitorCheck /> The device must acknowledge within 10 seconds</span></div></>}{error && <p className="web-inline-error"><CircleAlert /> {error}</p>}<footer><button type="button" className="web-button web-button-quiet" onClick={() => dialogRef.current?.close()}>Cancel</button><button type="submit" className="web-button web-button-primary" disabled={busy || !deviceId || !effectiveLibraryId || !versionId}>{busy ? <LoaderCircle className="is-spinning" /> : <Download />} Send to launcher</button></footer></form></dialog>
}

function AppPage() {
  const [session, setSession] = useState<WebSession | null>(null)
  const [error, setError] = useState('')
  const loadSession = useCallback(async () => {
    try {
      setSession(await webApi.session())
      setError('')
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'SESSION_FAILED')
      setSession({ enabled: true, authenticated: false })
    }
  }, [])
  useEffect(() => {
    const timer = window.setTimeout(() => void loadSession(), 0)
    return () => window.clearTimeout(timer)
  }, [loadSession])

  if (!session) return <main className="web-auth-state"><LoaderCircle className="is-spinning" /><h1>Checking your session</h1></main>
  if (!session.enabled) return <main className="web-auth-state"><CircleAlert /><p className="web-eyebrow">PREVIEW NOT ENABLED</p><h1>Remote Web is not enabled on this deployment.</h1><p>The public website remains available. Remote access opens only after the Render feature flags and canary account are configured.</p><RouteLink path="/" className="web-button web-button-quiet">Return home</RouteLink></main>
  if (!session.authenticated) return <main className="web-auth-state"><div className="web-auth-symbol"><LockKeyhole /></div><p className="web-eyebrow">VERIFIED REMOTE ACCESS</p><h1>Sign in with Discord</h1><p>Use the same Discord account that is authorized in your desktop launcher. A Discord user ID typed by hand is never accepted.</p>{error && <p className="web-inline-error"><CircleAlert /> {error}</p>}<a className="web-button web-button-primary" href={webApi.loginUrl('/app')}><ExternalLink /> Continue with Discord</a><small>OAuth Authorization Code + PKCE · HttpOnly session · no Discord token in browser storage</small></main>
  if (!session.legal?.accepted) return <LegalGate session={session} onAccepted={loadSession} />
  return <RemoteDashboard session={session} refreshSession={loadSession} />
}

function AuthErrorPage() {
  const rawCode = new URLSearchParams(window.location.search).get('code') || 'OAUTH_FAILED'
  const code = /^[A-Z0-9_]{1,80}$/.test(rawCode) ? rawCode : 'OAUTH_FAILED'
  return <main className="web-auth-state"><div className="web-auth-symbol"><CircleAlert /></div><p className="web-eyebrow">SIGN-IN INTERRUPTED</p><h1>Discord sign-in could not be completed.</h1><p>No remote request was created. Return to the dashboard and start a new secure sign-in flow.</p><code>{code}</code><RouteLink path="/app" className="web-button web-button-primary">Try again</RouteLink></main>
}

export function WebApp() {
  const [route, setRoute] = useState<PublicRoute>(() => normalizeRoute(window.location.pathname))
  useEffect(() => { const listener = () => setRoute(normalizeRoute(window.location.pathname)); window.addEventListener('popstate', listener); return () => window.removeEventListener('popstate', listener) }, [])

  if (route === '/app') return <AppPage />
  return <div className="web-site"><PublicHeader route={route} />{route === '/' ? <LandingPage /> : route === '/features' ? <FeaturesPage /> : route === '/download' ? <DownloadPage /> : route === '/auth/error' ? <AuthErrorPage /> : <DocumentPage route={route as keyof typeof DOCUMENTS} />}</div>
}
