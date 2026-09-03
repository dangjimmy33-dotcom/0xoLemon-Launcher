import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const toastSource = await readFile(new URL('./AchievementToast.tsx', import.meta.url), 'utf8')
const busSource = await readFile(new URL('../lib/achievementEventBus.ts', import.meta.url), 'utf8')
const platformSource = await readFile(new URL('../../src-tauri/src/platform.rs', import.meta.url), 'utf8')
const sessionSource = await readFile(new URL('../../src-tauri/src/game_session_state.rs', import.meta.url), 'utf8')

function toCamelCase(value) {
  return value.replace(/_([a-z])/g, (_match, letter) => letter.toUpperCase())
}

test('shared achievement bus normalizes legacy payload and toast consumes canonical v2', () => {
  const rustEvent = platformSource.match(
    /#\[serde\(rename_all = "camelCase"\)\]\s*pub struct AchievementUnlockedEvent\s*\{([\s\S]*?)\n\}/,
  )
  assert.ok(rustEvent, 'Rust must serialize AchievementUnlockedEvent with camelCase field names')

  const emittedFields = [...rustEvent[1].matchAll(/pub\s+([a-z_]+):/g)]
    .map((match) => toCamelCase(match[1]))
  assert.deepEqual(emittedFields, ['gameId', 'id', 'name', 'description', 'unlockedAt'])

  const consumerEvent = busSource.match(/type LegacyAchievementUnlockedEvent\s*=\s*\{([\s\S]*?)\n\}/)
  assert.ok(consumerEvent, 'shared bus must declare the legacy payload contract')
  for (const field of emittedFields) {
    assert.match(consumerEvent[1], new RegExp(`\\b${field}\\s*:`), `consumer is missing ${field}`)
  }

  assert.match(sessionSource, /pub struct AchievementEventV2/)
  assert.match(sessionSource, /"launcher:\/\/achievement-event-v2"/)
  assert.match(busSource, /listen<AchievementEventV2>\('launcher:\/\/achievement-event-v2'/)
  assert.match(toastSource, /subscribeAchievementEvents/)
  assert.match(toastSource, /event\.gameId/)
  assert.match(toastSource, /event\.achievementId/)
  assert.doesNotMatch(toastSource, /listen<AchievementUnlockedEvent>/)
  assert.doesNotMatch(toastSource, /\b(?:game_id|achievement_id|unlocked_at)\b/)
})
