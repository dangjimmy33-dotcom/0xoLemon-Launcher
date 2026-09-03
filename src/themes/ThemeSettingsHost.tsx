import { lazy, Suspense } from 'react'
import { SettingsView, type SettingsViewProps } from '../components/SettingsView'
import type { UiThemeId } from '../lib/uiThemes'
import { THEME_PACKAGES } from './contracts'

const SteamSettingsView = lazy(() => {
  const loader = THEME_PACKAGES.steam.loadSettingsView
  return loader ? loader() : Promise.resolve({ default: SettingsView })
})

function SettingsLoadingState() {
  return (
    <section className="theme-settings-loading" aria-label="Loading settings">
      <span />
      <span />
      <span />
    </section>
  )
}

export function ThemeSettingsHost({ theme, ...props }: SettingsViewProps & { theme: UiThemeId }) {
  if (theme !== 'steam') {
    return <SettingsView {...props} />
  }

  return (
    <Suspense fallback={<SettingsLoadingState />}>
      <SteamSettingsView {...props} />
    </Suspense>
  )
}
