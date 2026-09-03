import { Component, useEffect, useState, type ComponentType, type ErrorInfo, type ReactNode } from 'react'
import type { UiThemeId } from '../lib/uiThemes'
import {
  DEFAULT_THEME_ROUTE_FRAME,
  DEFAULT_THEME_SHELL,
  THEME_PACKAGES,
  type ThemeRouteFrameProps,
  type ThemeShellProps,
} from './contracts'

const loadedShells = new Map<UiThemeId, ComponentType<ThemeShellProps>>([
  ['default', DEFAULT_THEME_SHELL],
])
const loadedRouteFrames = new Map<UiThemeId, ComponentType<ThemeRouteFrameProps>>([
  ['default', DEFAULT_THEME_ROUTE_FRAME],
])

class ThemePackageBoundary extends Component<{
  theme: UiThemeId
  fallbackProps: ThemeShellProps
  onFailure?: (theme: UiThemeId) => void
  children: ReactNode
}, { failed: boolean }> {
  state = { failed: false }

  static getDerivedStateFromError() {
    return { failed: true }
  }

  componentDidUpdate(previousProps: Readonly<{ theme: UiThemeId }>) {
    if (previousProps.theme !== this.props.theme && this.state.failed) {
      this.setState({ failed: false })
    }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    document.documentElement.setAttribute('data-ui-theme', 'default')
    this.props.onFailure?.(this.props.theme)
    window.dispatchEvent(new CustomEvent('0xo-theme-package-error', {
      detail: { theme: this.props.theme, message: error.message, componentStack: info.componentStack },
    }))
  }

  render() {
    if (this.state.failed) return <DEFAULT_THEME_SHELL {...this.props.fallbackProps} />
    return this.props.children
  }
}

export function ThemeShellHost(props: ThemeShellProps & {
  theme: UiThemeId
  onThemeReady?: (theme: UiThemeId) => void
}) {
  const [activePackage, setActivePackage] = useState<{ theme: UiThemeId; Shell: ComponentType<ThemeShellProps> }>(() => ({
    theme: 'default',
    Shell: DEFAULT_THEME_SHELL,
  }))
  const [RouteFrame, setRouteFrame] = useState<ComponentType<ThemeRouteFrameProps>>(() => DEFAULT_THEME_ROUTE_FRAME)

  useEffect(() => {
    let canceled = false
    const cached = loadedShells.get(props.theme)
    const cachedFrame = loadedRouteFrames.get(props.theme)
    const shellPromise = cached
      ? Promise.resolve({ default: cached })
      : THEME_PACKAGES[props.theme].loadShell()
    const framePromise = cachedFrame
      ? Promise.resolve({ default: cachedFrame })
      : THEME_PACKAGES[props.theme].loadRouteFrame()

    void Promise.all([shellPromise, framePromise]).then(([shellModule, frameModule]) => {
      if (canceled) return
      loadedShells.set(props.theme, shellModule.default)
      loadedRouteFrames.set(props.theme, frameModule.default)
      setActivePackage({ theme: props.theme, Shell: shellModule.default })
      setRouteFrame(() => frameModule.default)
      props.onThemeReady?.(props.theme)
    }).catch((error) => {
      if (canceled) return
      setActivePackage({ theme: 'default', Shell: DEFAULT_THEME_SHELL })
      setRouteFrame(() => DEFAULT_THEME_ROUTE_FRAME)
      props.onThemeReady?.('default')
      window.dispatchEvent(new CustomEvent('0xo-theme-package-error', {
        detail: { theme: props.theme, message: error instanceof Error ? error.message : String(error) },
      }))
    })

    return () => { canceled = true }
  }, [props.onThemeReady, props.theme])

  const renderedPackage = props.theme === 'default'
    ? { theme: 'default' as const, Shell: DEFAULT_THEME_SHELL }
    : activePackage
  const Shell = renderedPackage.Shell
  const ActiveRouteFrame = renderedPackage.theme === props.theme ? RouteFrame : DEFAULT_THEME_ROUTE_FRAME
  const framedChildren = <ActiveRouteFrame {...props}>{props.children}</ActiveRouteFrame>
  return (
    <ThemePackageBoundary
      key={renderedPackage.theme}
      theme={renderedPackage.theme}
      fallbackProps={props}
      onFailure={() => props.onThemeReady?.('default')}
    >
      <Shell {...props}>{framedChildren}</Shell>
    </ThemePackageBoundary>
  )
}
