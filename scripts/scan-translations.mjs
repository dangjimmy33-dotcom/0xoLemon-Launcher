import https from 'node:https'
import fs from 'node:fs'

const repos = [
  'JOINCANE/0XoLemon',
  'CatManga/Cat-Manga',
  'Penaldo-CR7/PenaldoCR7',
  'PROBBI/PROBBINE',
  'Chat-stories/Chat-stories',
  'Akatsuki-tusu/naruto',
  'Immaking/Luas',
]

const TOKEN = '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'

function fetchJson(url) {
  return new Promise((resolve) => {
    const req = https.get(url, { headers: { 'User-Agent': '007Launcher', 'Authorization': `Bearer ${TOKEN}` }, timeout: 10000 }, (res) => {
      let data = ''
      res.on('data', (chunk) => (data += chunk))
      res.on('end', () => {
        try {
          resolve(JSON.parse(data))
        } catch {
          resolve(null)
        }
      })
    })
    req.on('error', () => resolve(null))
    req.on('timeout', () => { req.destroy(); resolve(null) })
  })
}

function formatSize(bytes) {
  if (!bytes) return 'N/A'
  const mb = bytes / (1024 * 1024)
  if (mb >= 1024) return (mb / 1024).toFixed(2) + ' GB'
  return mb.toFixed(1) + ' MB'
}

function cleanGameTitle(folder) {
  return folder.replace(/[_-]/g, ' ').replace(/\s+/g, ' ').trim()
}

async function run() {
  const map = new Map()

  for (const repo of repos) {
    console.log('Fetching repo tree:', repo)
    const rootTree = await fetchJson('https://huggingface.co/api/datasets/' + repo + '/tree/main')
    if (!Array.isArray(rootTree)) continue

    const dirs = rootTree.filter(item => item.type === 'directory')

    // Concurrently fetch in chunks of 15
    const chunkSize = 15
    for (let i = 0; i < dirs.length; i += chunkSize) {
      const chunk = dirs.slice(i, i + chunkSize)
      await Promise.all(chunk.map(async (item) => {
        const gameFolder = item.path
        const subTree = await fetchJson(
          'https://huggingface.co/api/datasets/' + repo + '/tree/main/' + encodeURIComponent(gameFolder),
        )
        if (!Array.isArray(subTree)) return

        for (const sub of subTree) {
          if (sub.type === 'directory' && (sub.path.toLowerCase().includes('viethoa') || sub.path.toLowerCase().includes('việt') || sub.path.toLowerCase().includes('patch'))) {
            const files = await fetchJson(
              'https://huggingface.co/api/datasets/' + repo + '/tree/main/' + encodeURIComponent(sub.path),
            )
            if (Array.isArray(files)) {
              for (const f of files) {
                if (f.path.endsWith('.7z') || f.path.endsWith('.zip') || f.path.endsWith('.rar')) {
                  const fileName = f.path.split('/').pop() || ''
                  const key = repo + ':' + f.path
                  map.set(key, {
                    id: 'vh-' + Buffer.from(key).toString('hex').slice(0, 10),
                    gameTitle: cleanGameTitle(gameFolder),
                    translationTitle: fileName.replace(/\.(7z|zip|rar)$/i, '').replace(/[._-]+/g, ' ').trim(),
                    fileName,
                    author: '',
                    version: '',
                    size: formatSize(f.size),
                    downloadUrl: 'https://huggingface.co/datasets/' + repo + '/resolve/main/' + f.path,
                    coverUrl: '',
                    description: '',
                    installGuide: '',
                    tags: [],
                    repo,
                  })
                }
              }
            }
          } else if (sub.type === 'file' && (sub.path.endsWith('.7z') || sub.path.endsWith('.zip') || sub.path.endsWith('.rar')) && (sub.path.toLowerCase().includes('viethoa') || sub.path.toLowerCase().includes('vh') || sub.path.toLowerCase().includes('viet'))) {
            const fileName = sub.path.split('/').pop() || ''
            const key = repo + ':' + sub.path
            map.set(key, {
              id: 'vh-' + Buffer.from(key).toString('hex').slice(0, 10),
              gameTitle: cleanGameTitle(gameFolder),
              translationTitle: fileName.replace(/\.(7z|zip|rar)$/i, '').replace(/[._-]+/g, ' ').trim(),
              fileName,
              author: '',
              version: '',
              size: formatSize(sub.size),
              downloadUrl: 'https://huggingface.co/datasets/' + repo + '/resolve/main/' + sub.path,
              coverUrl: '',
              description: '',
              installGuide: '',
              tags: [],
              repo,
            })
          }
        }
      }))
    }
  }

  const list = Array.from(map.values()).sort((a, b) => a.gameTitle.localeCompare(b.gameTitle))
  console.log('Total extracted:', list.length)

  const fileContent = `export interface VietnameseTranslationItem {
  id: string
  gameTitle: string
  translationTitle: string
  fileName?: string
  author: string
  version: string
  size: string
  downloadUrl: string
  coverUrl?: string
  bannerUrl?: string
  description: string
  installGuide?: string
  tags: string[]
  repo?: string
  gameId?: string
}

export const VIETNAMESE_TRANSLATIONS_DATA: VietnameseTranslationItem[] = ${JSON.stringify(list, null, 2)};
`

  fs.writeFileSync('src/data/vietnameseTranslations.ts', fileContent, 'utf8')
  console.log('Wrote to src/data/vietnameseTranslations.ts successfully!')
}

run()
