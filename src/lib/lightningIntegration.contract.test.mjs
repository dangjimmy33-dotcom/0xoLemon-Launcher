import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const types = readFileSync(new URL('../types.ts', import.meta.url), 'utf8')
const app = readFileSync(new URL('../App.tsx', import.meta.url), 'utf8')
const activeView = readFileSync(new URL('../components/ActiveView.tsx', import.meta.url), 'utf8')
const compatibilityEntry = readFileSync(new URL('../components/LightningHub.tsx', import.meta.url), 'utf8')
const toolsView = readFileSync(new URL('../components/GameToolsHub.tsx', import.meta.url), 'utf8')
const toolsWorkspace = readFileSync(new URL('../components/cinematic/ToolsWorkspaceView.tsx', import.meta.url), 'utf8')
const providerView = readFileSync(new URL('../components/cinematic/BypassProviderView.tsx', import.meta.url), 'utf8')
const providerMeta = readFileSync(new URL('../components/cinematic/bypassProviders.ts', import.meta.url), 'utf8')
const catalogView = readFileSync(new URL('../components/cinematic/BypassCatalogView.tsx', import.meta.url), 'utf8')
const detailView = readFileSync(new URL('../components/cinematic/BypassGameDetailView.tsx', import.meta.url), 'utf8')
const newsView = readFileSync(new URL('../components/cinematic/CinematicNewsView.tsx', import.meta.url), 'utf8')
const shell = readFileSync(new URL('../themes/lightning/LightningShell.tsx', import.meta.url), 'utf8')
const navigation = readFileSync(new URL('./launcherNavigation.ts', import.meta.url), 'utf8')
const rust = readFileSync(new URL('../../src-tauri/src/lightning_integration.rs', import.meta.url), 'utf8')
const importer = readFileSync(new URL('../../src-tauri/src/game_tools_import.rs', import.meta.url), 'utf8')
const wrappers = readFileSync(new URL('../../src-tauri/src/game_tools.rs', import.meta.url), 'utf8')
const transaction = readFileSync(new URL('../../src-tauri/src/managed_file_transaction.rs', import.meta.url), 'utf8')
const invokeHandler = readFileSync(new URL('../../src-tauri/src/lib.rs', import.meta.url), 'utf8')
const permissions = readFileSync(new URL('../../src-tauri/permissions/allow-all.json', import.meta.url), 'utf8')

const neutralCommands = [
  'get_game_tools_catalog',
  'get_game_tools_status',
  'import_game_tools_files',
  'get_game_tools_package_identity',
  'pick_game_tools_install_dir',
  'apply_game_tools_package',
  'restore_latest_game_tools_package',
  'get_game_tools_game_status',
  'list_game_tools_executables',
  'apply_game_tools_steamless',
  'restore_game_tools_steamless',
]

for (const command of neutralCommands) {
  assert.ok(invokeHandler.includes(`game_tools::${command}`), `${command} must be registered in the Tauri invoke handler`)
  assert.ok(permissions.includes(`"${command}"`), `${command} must be granted by the desktop ACL`)
  assert.ok(wrappers.includes(`fn ${command}`), `${command} must be implemented by the neutral GameTools adapter`)
}

for (const legacyCommand of ['get_lightning_catalog', 'get_lightning_integration_status', 'apply_lightning_package']) {
  assert.ok(invokeHandler.includes(`lightning_integration::${legacyCommand}`), `${legacyCommand} must remain for one compatibility cycle`)
}

assert.ok(types.includes("| 'Tools'"), 'Game Tools must be a typed launcher route')
assert.ok(app.includes("'Lua Shop', 'Tools'] as const"), 'Tools must be accepted by root navigation')
assert.ok(activeView.includes("activeTab === 'Tools'"), 'Tools must have a real active view')
assert.ok(activeView.includes("const GameToolsView = lazy(() => import('./LightningHub'))"), 'the compatibility chunk name must remain lazy-loaded for one cycle')
assert.ok(compatibilityEntry.includes("from './GameToolsHub'"), 'LightningHub must be a thin migration entrypoint')
assert.ok(navigation.includes("candidate.tab === 'Lightning Hub' ? 'Tools'"), 'old navigation snapshots must migrate to Tools')

for (const label of ['Home', 'Nexus', 'Library', 'Instant Gaming', 'Tools', 'Bypass', 'OnlineFix', 'Settings']) {
  assert.ok(shell.includes(`label: '${label}'`), `${label} must be a first-class Cinematic destination`)
}
assert.ok(!shell.includes("label: 'Lightning Tools'"), 'visible navigation must not retain the old feature name')

for (const renderer of [newsView, toolsWorkspace, providerView, catalogView, detailView]) {
  assert.ok(renderer.length > 500, 'each Cinematic renderer must be implemented independently')
}
assert.ok(newsView.includes('STORY_INTERVAL_MS = 6_000'), 'Last News must rotate every six seconds')
assert.ok(newsView.includes("scrollIntoView({ behavior: 'smooth'"), 'the selected story must center itself in the filmstrip')
assert.ok(toolsWorkspace.includes("ALLOWED_SUFFIXES = ['.lua', '.zip', '.manifest']"), 'Tools drag/drop must expose only the supported import types')
for (const provider of ['Ubisoft', 'EA', 'Rockstar', 'Denuvo', 'PlayStation', 'Other']) {
  assert.ok((providerView + providerMeta).includes(provider), `provider metadata missing: ${provider}`)
}
assert.ok(!detailView.includes('role="dialog"'), 'game detail must be a full route rather than a modal/drawer')

const requestType = types.match(/export type GameToolsPackageRequest = \{([\s\S]*?)\n\}/)?.[1] ?? ''
for (const field of ['appId: number', 'requestId: string', 'installDir: string', 'revision: string', 'packageSha256: string']) {
  assert.ok(requestType.includes(field), `package request field missing: ${field}`)
}
assert.ok(toolsView.indexOf("invoke<string | null>('pick_game_tools_install_dir'") < toolsView.indexOf("invoke<GameToolsSourceIdentity>('get_game_tools_package_identity'"), 'the native picker must run before source locking')
assert.ok(toolsView.includes("if (!installDir) return"), 'canceling the picker must create no package job')
assert.ok(toolsView.includes("invoke<GameToolsPackageResult>('apply_game_tools_package'"), 'Apply Fix must call the neutral transactional command')
assert.ok(rust.includes('request.install_dir.trim()'), 'the backend must canonicalize the user-selected directory')
assert.ok(rust.includes('request.revision.trim() != source.reference'), 'the source revision must be locked before apply')
assert.ok(rust.includes('normalize_package_sha256(&request.package_sha256)'), 'the package descriptor hash must be verified')
assert.ok(rust.includes('crate::managed_file_transaction::apply_files'), 'package apply must use the shared managed transaction')
assert.ok(rust.includes('crate::managed_file_transaction::apply_changes'), 'package restore must use the shared managed transaction')
assert.ok(transaction.includes('expected_sha256'), 'the shared transaction must verify staged hashes')

for (const guard of ['MAX_IMPORT_SOURCES', 'MAX_SOURCE_BYTES', 'MAX_ARCHIVE_ENTRIES', 'MAX_EXPANDED_BYTES', 'enclosed_name', 'GAME_TOOLS_IMPORT_DUPLICATE_TARGET']) {
  assert.ok(importer.includes(guard), `import hardening guard missing: ${guard}`)
}
assert.ok(importer.includes('ManagedFileSpec'), 'imports must commit through ManagedFileTransaction')
assert.ok(toolsView.includes("listen<GameToolsPackageProgress>('launcher://game-tools-package-progress'"), 'package progress must use the neutral event')
assert.ok(!toolsWorkspace.includes('Steamless'), 'Steamless must not be rendered inside Tools workspace')
assert.ok(!toolsWorkspace.includes('list_sff_feature_packages'), 'large components must stay in Settings')

console.log('lightningIntegration.contract: PASS')
