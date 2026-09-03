import { useCallback } from 'react'
import { useSocialPrototype } from '../social/SocialProvider'
import { ThemeShellHost } from './ThemeShellHost'
import type { ThemeShellProps } from './contracts'
import type { UiThemeId } from '../lib/uiThemes'

type ConnectedThemeShellHostProps = Omit<ThemeShellProps, 'onOpenSelfProfile'> & {
  theme: UiThemeId
  onThemeReady?: (theme: UiThemeId) => void
}

export function ConnectedThemeShellHost(props: ConnectedThemeShellHostProps) {
  const { selfId, openFullProfile } = useSocialPrototype()
  const { onNavigate } = props
  const openSelfProfile = useCallback(() => {
    onNavigate('Social')
    if (selfId) openFullProfile(selfId)
  }, [onNavigate, openFullProfile, selfId])

  return <ThemeShellHost {...props} onOpenSelfProfile={openSelfProfile} />
}
