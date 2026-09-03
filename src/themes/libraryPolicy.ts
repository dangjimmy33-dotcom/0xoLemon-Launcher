import type { UiThemeId } from '../lib/uiThemes'

export type LibraryViewMode = 'store' | 'library'

export type ThemeLibraryPresentation = {
  acquisitionOnlyStore: boolean
  ownershipLibrary: boolean
  showLibraryRail: boolean
  showSourceSwitches: boolean
}

export function getThemeLibraryPresentation(
  theme: UiThemeId,
  viewMode: LibraryViewMode,
): ThemeLibraryPresentation {
  const steam = theme === 'steam'
  return {
    acquisitionOnlyStore: steam && viewMode === 'store',
    ownershipLibrary: steam && viewMode === 'library',
    showLibraryRail: steam && viewMode === 'library',
    showSourceSwitches: !steam,
  }
}
