import fs from 'node:fs'
import path from 'node:path'
import test from 'node:test'
import assert from 'node:assert/strict'

const root = path.resolve(import.meta.dirname, '..', '..')
const read = (relativePath) => fs.readFileSync(path.join(root, relativePath), 'utf8')

function rustFunction(source, name) {
  const start = source.indexOf(`fn ${name}`)
  assert.notEqual(start, -1, `missing Rust function ${name}`)
  const nextCommand = source.indexOf('#[tauri::command]', start + 1)
  return source.slice(start, nextCommand === -1 ? source.length : nextCommand)
}

test('cover selection is a disposable draft and profile Save is the only commit point', () => {
  const api = read('src/social/socialApi.ts')
  const provider = read('src/social/SocialProvider.tsx')
  const editor = read('src/social/SocialPrototype.tsx')
  const rust = read('src-tauri/src/social.rs')
  const commands = read('src-tauri/src/lib.rs')

  for (const command of [
    'stage_social_cover',
    'commit_social_profile_draft',
    'discard_social_profile_draft',
    'retry_social_cover_publish',
  ]) {
    assert.match(rust, new RegExp(`fn\\s+${command}\\b`), `Rust is missing ${command}`)
    assert.match(commands, new RegExp(`social::${command}\\b`), `${command} is not registered with Tauri`)
  }

  assert.match(api, /stageSocialCover[\s\S]*'stage_social_cover'/, 'frontend must invoke stage_social_cover')
  assert.match(api, /commitSocialProfileDraft[\s\S]*'commit_social_profile_draft'/, 'frontend must invoke commit_social_profile_draft')
  assert.match(api, /discardSocialProfileDraft[\s\S]*'discard_social_profile_draft'/, 'frontend must invoke discard_social_profile_draft')

  const stage = rustFunction(rust, 'stage_social_cover')
  assert.doesNotMatch(stage, /local_cover_path\s*\(/, 'staging must not replace the canonical cover')
  assert.doesNotMatch(stage, /pending_cover_path\s*\(/, 'staging must not enqueue a public upload')

  const saveStart = provider.indexOf('const saveSelfProfile')
  const chooseStart = provider.indexOf('const chooseSelfCover', saveStart)
  const save = provider.slice(saveStart, chooseStart)
  assert.doesNotMatch(save, /snapshot\.coverBytes|applyLocalProfileSnapshot\s*\(snapshot\)/, 'Save must never reload the legacy cover.bin')
  assert.match(save, /commitSocialProfileDraft\s*\(/, 'Save must commit the staged cover draft')

  assert.match(editor, /discardSelfCoverDraft/, 'Cancel/close must discard the current exact draft')
  assert.match(editor, /saveSelfProfile\s*\(draft,\s*coverOperation\)/, 'editor Save must commit the selected cover operation')
})

test('canonical or removed cover state prevents legacy cover resurrection', () => {
  const provider = read('src/social/SocialProvider.tsx')
  const rust = read('src-tauri/src/social.rs')

  assert.match(rust, /cover_removed_tombstone_path/, 'remove must persist a tombstone')
  assert.match(provider, /migrateLegacySocialCover/, 'provider must explicitly migrate a legacy cover once')
  assert.match(provider, /state\.localPath\s*\|\|\s*state\.removed/, 'canonical/tombstoned state must win over legacy bytes')
  assert.doesNotMatch(provider, /if\s*\(snapshot\.coverBytes\?\.byteLength\)\s*applyLocalProfileSnapshot/, 'legacy bytes must not be reapplied during Save')
})
