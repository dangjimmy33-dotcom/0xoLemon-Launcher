import { readFile, readdir } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const sourceRoot = path.join(root, 'src')
const permissionPath = path.join(root, 'src-tauri', 'permissions', 'allow-all.json')
const rustEntryPath = path.join(root, 'src-tauri', 'src', 'lib.rs')

const requiredHandlerCommands = [
  'discover_game_installs',
  'register_library_root',
  'forget_library_root',
  'resolve_install_conflict',
  'get_lua_game_manager_state',
  'resolve_lua_source',
  'set_lua_game_channel',
  'sync_lua_game',
  'scan_lua_sources',
  'install_lua_game_from_source',
  'sync_lua_game_from_source',
  'apply_lua_game_update',
  'check_lua_game_update',
  'check_all_lua_game_updates',
]

async function collectSourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true })
  const files = []
  for (const entry of entries) {
    const fullPath = path.join(directory, entry.name)
    if (entry.isDirectory()) {
      files.push(...await collectSourceFiles(fullPath))
    } else if (/\.(?:ts|tsx)$/.test(entry.name) && !entry.name.endsWith('.d.ts')) {
      files.push(fullPath)
    }
  }
  return files
}

function literalInvokeCommands(source, filePath) {
  const scriptKind = filePath.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS
  const sourceFile = ts.createSourceFile(filePath, source, ts.ScriptTarget.Latest, true, scriptKind)
  const commands = []

  function visit(node) {
    if (
      ts.isCallExpression(node)
      && ts.isIdentifier(node.expression)
      && node.expression.text === 'invoke'
      && node.arguments.length > 0
    ) {
      const argument = node.arguments[0]
      if (ts.isStringLiteralLike(argument) && !argument.text.startsWith('plugin:')) {
        commands.push(argument.text)
      }
    }
    ts.forEachChild(node, visit)
  }

  visit(sourceFile)
  return commands
}

function extractGenerateHandlerBody(source) {
  const marker = 'tauri::generate_handler!['
  const markerIndex = source.indexOf(marker)
  if (markerIndex < 0) {
    throw new Error(`Could not find ${marker} in ${rustEntryPath}`)
  }
  const openIndex = source.indexOf('[', markerIndex)
  let depth = 0
  for (let index = openIndex; index < source.length; index += 1) {
    if (source[index] === '[') depth += 1
    if (source[index] === ']') {
      depth -= 1
      if (depth === 0) return source.slice(openIndex + 1, index)
    }
  }
  throw new Error(`Could not parse generate_handler! body in ${rustEntryPath}`)
}

const [permissionText, rustEntry, sourceFiles] = await Promise.all([
  readFile(permissionPath, 'utf8'),
  readFile(rustEntryPath, 'utf8'),
  collectSourceFiles(sourceRoot),
])

const permissionDocument = JSON.parse(permissionText)
const allowed = new Set(permissionDocument?.permission?.[0]?.commands?.allow ?? [])
const invoked = new Set()
for (const filePath of sourceFiles) {
  const source = await readFile(filePath, 'utf8')
  for (const command of literalInvokeCommands(source, filePath)) invoked.add(command)
}

const missingAcl = [...invoked].filter((command) => !allowed.has(command)).sort()
const handlerBody = extractGenerateHandlerBody(rustEntry)
const missingHandler = requiredHandlerCommands.filter(
  (command) => !new RegExp(`\\b${command}\\b`).test(handlerBody),
)
const missingRequiredAcl = requiredHandlerCommands.filter((command) => !allowed.has(command))

if (missingAcl.length || missingHandler.length || missingRequiredAcl.length) {
  if (missingAcl.length) console.error(`Frontend commands missing from ACL: ${missingAcl.join(', ')}`)
  if (missingRequiredAcl.length) console.error(`Required commands missing from ACL: ${missingRequiredAcl.join(', ')}`)
  if (missingHandler.length) console.error(`Required commands missing from generate_handler!: ${missingHandler.join(', ')}`)
  process.exitCode = 1
} else {
  console.log(`Tauri ACL preflight passed (${invoked.size} literal frontend commands checked).`)
}
