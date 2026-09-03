import type { ComponentType, ReactNode } from 'react'
import type { TabId, XmclInstanceGroup } from '../types'
import { getUiThemeProfile, type UiThemeId, type UiThemeNativePalette } from '../lib/uiThemes'
import DefaultRouteFrame from './default/DefaultRouteFrame'
import DefaultShell from './default/DefaultShell'
import type { SettingsViewProps } from '../components/SettingsView'

export type ThemeMotionProfile = '0xolemon' | 'lightning-cinematic' | 'steam-desktop' | 'xmcl-instance'

export type ThemeCapabilities = {
  backForwardNavigation: boolean
  libraryCollections: boolean
  instanceGroups: boolean
  bottomStatusBar: boolean
}

export type ThemeInstanceViewModel = {
  gameId: string
  title: string
  iconUrl?: string
  gridUrl?: string
  heroUrl?: string
  developer?: string
  description?: string
  installed: boolean
  favorite: boolean
  playing: boolean
}

export type ThemeShellProps = {
  children: ReactNode
  activeTab: TabId
  onNavigate: (tab: TabId) => void
  onBack: () => void
  onForward: () => void
  canGoBack: boolean
  canGoForward: boolean
  serviceStatus: string
  updateCount: number
  downloadCount: number
  luaModeEnabled: boolean
  discordAuthorized: boolean
  displayName?: string | null
  selectedGameTitle?: string | null
  selectedGameId?: string | null
  instances: readonly ThemeInstanceViewModel[]
  instanceGroups: readonly XmclInstanceGroup[]
  onSelectGame: (gameId: string | null) => void
  onOpenSelfProfile: () => void
  isSidebarCollapsed: boolean
  onToggleSidebar: () => void
}

export type ThemeRouteFrameProps = Pick<ThemeShellProps,
  | 'children'
  | 'activeTab'
  | 'selectedGameId'
  | 'selectedGameTitle'
  | 'serviceStatus'
  | 'updateCount'
  | 'downloadCount'
>

export type ThemePackage = {
  id: UiThemeId
  referenceVersion: string
  nativePalette: UiThemeNativePalette
  motionProfile: ThemeMotionProfile
  capabilities: ThemeCapabilities
  routeRenderers: readonly TabId[] | 'all'
  overlayRenderers: readonly ('dialog' | 'contextMenu' | 'dropdown' | 'toast' | 'loading' | 'error')[]
  loadShell: () => Promise<{ default: ComponentType<ThemeShellProps> }>
  loadRouteFrame: () => Promise<{ default: ComponentType<ThemeRouteFrameProps> }>
  loadSettingsView?: () => Promise<{ default: ComponentType<SettingsViewProps> }>
}

export const DEFAULT_THEME_SHELL = DefaultShell
export const DEFAULT_THEME_ROUTE_FRAME = DefaultRouteFrame

export const THEME_PACKAGES: Record<UiThemeId, ThemePackage> = {
  default: {
    id: 'default',
    referenceVersion: '0xolemon-v2',
    nativePalette: getUiThemeProfile('default').nativePalette,
    motionProfile: '0xolemon',
    routeRenderers: 'all',
    overlayRenderers: ['dialog', 'contextMenu', 'dropdown', 'toast', 'loading', 'error'],
    capabilities: {
      backForwardNavigation: false,
      libraryCollections: true,
      instanceGroups: false,
      bottomStatusBar: false,
    },
    loadShell: async () => ({ default: DEFAULT_THEME_SHELL }),
    loadRouteFrame: async () => ({ default: DEFAULT_THEME_ROUTE_FRAME }),
  },
  lightning: {
    id: 'lightning',
    referenceVersion: 'project-lightning-v5.0.8-snapshot',
    nativePalette: getUiThemeProfile('lightning').nativePalette,
    motionProfile: 'lightning-cinematic',
    routeRenderers: 'all',
    overlayRenderers: ['dialog', 'contextMenu', 'dropdown', 'toast', 'loading', 'error'],
    capabilities: {
      backForwardNavigation: true,
      libraryCollections: true,
      instanceGroups: false,
      bottomStatusBar: true,
    },
    loadShell: () => import('./lightning/LightningShell'),
    loadRouteFrame: () => import('./lightning/LightningRouteFrame'),
  },
  steam: {
    id: 'steam',
    referenceVersion: 'steam-desktop-2026.08',
    nativePalette: getUiThemeProfile('steam').nativePalette,
    motionProfile: 'steam-desktop',
    routeRenderers: 'all',
    overlayRenderers: ['dialog', 'contextMenu', 'dropdown', 'toast', 'loading', 'error'],
    capabilities: {
      backForwardNavigation: true,
      libraryCollections: true,
      instanceGroups: false,
      bottomStatusBar: true,
    },
    loadShell: () => import('./steam/SteamShell'),
    loadRouteFrame: () => import('./steam/SteamRouteFrame'),
    loadSettingsView: () => import('./steam/SteamSettingsView'),
  },
  xmcl: {
    id: 'xmcl',
    referenceVersion: 'xmcl-source-2026.08',
    nativePalette: getUiThemeProfile('xmcl').nativePalette,
    motionProfile: 'xmcl-instance',
    routeRenderers: 'all',
    overlayRenderers: ['dialog', 'contextMenu', 'dropdown', 'toast', 'loading', 'error'],
    capabilities: {
      backForwardNavigation: true,
      libraryCollections: true,
      instanceGroups: true,
      bottomStatusBar: false,
    },
    loadShell: () => import('./xmcl/XmclShell'),
    loadRouteFrame: () => import('./xmcl/XmclRouteFrame'),
  },
}
