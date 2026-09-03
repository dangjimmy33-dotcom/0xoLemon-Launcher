import assert from 'node:assert/strict'
import { readFile, readdir } from 'node:fs/promises'
import path from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8')
const [main, desktopBootstrap, webBootstrap, webApp, webApi, app, gate, rules, rust, invokeHandler, acl, routes] = await Promise.all([
  read('../main.tsx'),
  read('../desktop/bootstrap.tsx'),
  read('../web/bootstrap.tsx'),
  read('../web/WebApp.tsx'),
  read('../web/api.ts'),
  read('../App.tsx'),
  read('../components/DiscordAccessGate.tsx'),
  read('../../firestore.rules'),
  read('../../src-tauri/src/remote_web.rs'),
  read('../../src-tauri/src/lib.rs'),
  read('../../src-tauri/permissions/allow-all.json'),
  read('../../backend-api/remote/routes.js'),
])

async function collectFrontendSource(directoryPath) {
  const entries = await readdir(directoryPath, { withFileTypes: true })
  const chunks = []
  for (const entry of entries) {
    const entryPath = path.join(directoryPath, entry.name)
    if (entry.isDirectory()) {
      chunks.push(await collectFrontendSource(entryPath))
    } else if (/\.(?:ts|tsx|js|jsx)$/.test(entry.name) && !/\.(?:test|spec)\./.test(entry.name)) {
      chunks.push(await readFile(entryPath, 'utf8'))
    }
  }
  return chunks.join('\n')
}

const frontendSource = await collectFrontendSource(fileURLToPath(new URL('../', import.meta.url)))

test('browser and desktop use separate dynamic entry points', () => {
  assert.match(main, /import\('\.\/desktop\/bootstrap'\)/)
  assert.match(main, /import\('\.\/web\/bootstrap'\)/)
  assert.match(desktopBootstrap, /import App from '\.\.\/App'/)
  assert.doesNotMatch(webBootstrap, /\.\.\/App/)
})

test('browser authentication only uses the Discord OAuth session flow', () => {
  assert.match(webApp, /Sign in with Discord/)
  assert.match(webApp, /Continue with Discord/)
  assert.match(webApi, /\/auth\/discord\/start/)
  assert.match(webApi, /credentials:\s*'include'/)
  assert.doesNotMatch(frontendSource, /enter your Discord User ID|Connect to PC|manual-discord-id|Object\.assign\(status/i)
})

test('legacy direct Firestore remote control cannot run', () => {
  assert.doesNotMatch(app, /FirebaseRemoteControl/)
  assert.doesNotMatch(app, /collection\(socialDb, 'users'/)
  assert.doesNotMatch(app, /action:\s*'install'[\s\S]*serverTimestamp/)
  assert.doesNotMatch(gate, /manual-discord-id|Object\.assign\(status|Copy User ID/)
  assert.match(rules, /match \/users\/\{userId\}\/\{document=\*\*\}[\s\S]*?allow read, write: if false/)
})

test('remote collections are backend-only and desktop commands are ACL-bound', () => {
  for (const collection of ['webSessions', 'legalAcceptances', 'launcherDevices', 'remoteJobs']) {
    assert.match(rules, new RegExp(`match \\/${collection}\\/\\{document=\\*\\*\\} \\{ allow read, write: if false; \\}`))
  }
  for (const command of ['get_remote_web_access_state', 'enable_remote_web_access', 'disable_remote_web_access', 'revoke_remote_web_access']) {
    assert.ok(invokeHandler.includes(`remote_web::${command}`), `${command} must be registered`)
    assert.ok(acl.includes(`"${command}"`), `${command} must be permitted`)
    assert.ok(rust.includes(`fn ${command}`), `${command} must be implemented`)
  }
})

test('web auth uses PKCE, server sessions, CSRF and typed remote jobs', () => {
  assert.match(routes, /code_challenge_method', 'S256'/)
  assert.match(routes, /httpOnly:\s*true/)
  assert.match(routes, /requireSession, requireCsrf/)
  assert.match(routes, /router\.post\('\/remote-jobs'/)
  assert.doesNotMatch(routes, /installPath|executable|rawUrl/)
})
