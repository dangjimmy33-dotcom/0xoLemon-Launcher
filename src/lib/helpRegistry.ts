import type { TabId } from '../types'

export type HelpTopicId =
  | 'whatsNew'
  | 'home'
  | 'store'
  | 'luaShop'
  | 'luaInstaller'
  | 'library'
  | 'offlineActivation'
  | 'updates'
  | 'downloads'
  | 'cloudRedirect'
  | 'translations'
  | 'cache'
  | 'settings'

export const HELP_TOPIC_BY_TAB: Record<TabId, HelpTopicId> = {
  "What's New!": 'whatsNew',
  'Home': 'home',
  'Store': 'store',
  'Lua Shop': 'luaShop',
  'Lua Installer': 'luaInstaller',
  'Library': 'library',
  'Offline Activation': 'offlineActivation',
  'Updates': 'updates',
  'Downloads': 'downloads',
  'CloudRedirect': 'cloudRedirect',
  'Translations': 'translations',
  'Cache': 'cache',
  'Settings': 'settings',
}

export const HELP_TOPIC_ORDER: HelpTopicId[] = [
  'home', 'store', 'library', 'luaShop', 'luaInstaller', 'updates', 'downloads',
  'cloudRedirect', 'translations', 'offlineActivation', 'cache', 'settings', 'whatsNew',
]

export type HelpConceptId =
  | 'buildId'
  | 'manifest'
  | 'depotKey'
  | 'verify'
  | 'cache'
  | 'cloudSave'
  | 'luaMode'
  | 'luaSources'
  | 'offlineActivation'

export const HELP_CONCEPT_ORDER: HelpConceptId[] = [
  'buildId', 'manifest', 'depotKey', 'verify', 'cache', 'cloudSave', 'luaMode', 'luaSources', 'offlineActivation',
]
