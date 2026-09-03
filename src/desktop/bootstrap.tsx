import { Component, StrictMode, type ErrorInfo, type ReactNode } from 'react'
import { createRoot } from 'react-dom/client'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import App from '../App'
import Overlay from '../Overlay'
import { LocaleProvider } from '../context/LocaleContext'
import { isTauriRuntime } from '../lib/tauriRuntime'
import '../index.css'

type LauncherErrorBoundaryState = { error: Error | null }

class LauncherErrorBoundary extends Component<{ children: ReactNode }, LauncherErrorBoundaryState> {
  state: LauncherErrorBoundaryState = { error: null }

  static getDerivedStateFromError(error: Error): LauncherErrorBoundaryState {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('Launcher render failed:', error, info.componentStack)
  }

  render() {
    if (!this.state.error) return this.props.children
    return (
      <main className="desktop-bootstrap-error">
        <section>
          <h1>Launcher UI error / Lỗi giao diện Launcher</h1>
          <p>The launcher stayed open so this error can be diagnosed instead of leaving a black window.</p>
          <pre>{this.state.error.stack || this.state.error.message}</pre>
        </section>
      </main>
    )
  }
}

async function clearLegacyPwaState() {
  try {
    if ('serviceWorker' in navigator) {
      const registrations = await navigator.serviceWorker.getRegistrations()
      await Promise.all(registrations.map((registration) => registration.unregister()))
    }
    if ('caches' in window) {
      const cacheNames = await caches.keys()
      await Promise.all(cacheNames.map((cacheName) => caches.delete(cacheName)))
    }
  } catch (error) {
    console.warn('Unable to clear legacy desktop PWA state:', error)
  }
}

async function resolveSurface(isOverlay: boolean): Promise<ReactNode> {
  if (isOverlay) return <Overlay />

  const fixture = import.meta.env.DEV
    ? new URLSearchParams(window.location.search).get('fixture')
    : null

  if (fixture === 'whats-new') {
    const { WhatsNewView } = await import('../components/WhatsNewView')
    return <div style={{ width: '100vw', height: '100vh', overflow: 'hidden' }}><WhatsNewView /></div>
  }
  if (fixture === 'default-home') {
    const { default: DefaultHomeFixture } = await import('../themes/default/DefaultHomeFixture')
    return <DefaultHomeFixture />
  }
  if (fixture === 'steam-launch-options') {
    const { default: SteamLaunchOptionsDialog } = await import('../components/SteamLaunchOptionsDialog')
    return <SteamLaunchOptionsDialog locale="en-US" initialAppId={null} onClose={() => undefined} />
  }
  if (fixture === 'translations') {
    const { default: TranslationsFixture } = await import('../components/TranslationsFixture')
    return <TranslationsFixture />
  }
  if (fixture === 'theme-lightning' || fixture === 'theme-steam' || fixture === 'theme-xmcl') {
    const { default: ThemeFixture } = await import('../themes/ThemeFixture')
    return <ThemeFixture theme={fixture.replace('theme-', '') as 'lightning' | 'steam' | 'xmcl'} />
  }
  return <App />
}

export async function bootstrapDesktop() {
  let isOverlay = false
  try {
    isOverlay = getCurrentWebviewWindow().label === 'overlay'
  } catch {
    // Keep the main window when querying the label fails during startup.
  }

  if (isOverlay) document.body.classList.add('is-overlay-window')
  const surface = await resolveSurface(isOverlay)

  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <LauncherErrorBoundary>
        <LocaleProvider>{surface}</LocaleProvider>
      </LauncherErrorBoundary>
    </StrictMode>,
  )

  void clearLegacyPwaState()

  document.addEventListener('contextmenu', (event) => {
    const target = event.target as HTMLElement
    if (target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA') return
    event.preventDefault()
  })

  if (isTauriRuntime()) {
    const current = getCurrentWebviewWindow()
    // The diagnostic overlay is created hidden and must only become visible in
    // response to its Shift+F1 owner. Showing/focusing it during bootstrap steals
    // input from the game and made the fallback compete with native Shift+Tab.
    if (!isOverlay) {
      await current.show().catch(() => undefined)
      await current.unminimize().catch(() => undefined)
      await current.setFocus().catch(() => undefined)
    }
  }
}
