// Compatibility entrypoint for one release cycle. New code imports the neutral
// GameTools renderer; the legacy filename remains so cached lazy chunks migrate.
export { GameToolsView, default } from './GameToolsHub'
