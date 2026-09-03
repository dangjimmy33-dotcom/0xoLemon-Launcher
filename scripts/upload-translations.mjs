import https from 'node:https'
import fs from 'node:fs'

const TOKEN = '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'
const REPO = 'JOINCANE/0XoLemon'
const FILE_PATH = 'translations.json'

const content = fs.readFileSync('translations.json', 'utf8')
const contentBuffer = Buffer.from(content, 'utf8')

// Use HuggingFace HTTP API: PUT /api/repos/{repoType}/{repoId}/raw/{filePath}
// Correct endpoint for file upload: /api/datasets/{owner}/{repo}/raw/{branch}/{file}
const data = contentBuffer

const options = {
  hostname: 'huggingface.co',
  path: `/api/datasets/${REPO}/raw/main/${FILE_PATH}`,
  method: 'PUT',
  headers: {
    Authorization: `Bearer ${TOKEN}`,
    'Content-Type': 'application/json',
    'Content-Length': data.length,
  },
}

console.log('Uploading to:', `https://huggingface.co${options.path}`)

const req = https.request(options, (res) => {
  let body = ''
  res.on('data', chunk => body += chunk)
  res.on('end', () => {
    console.log('Status:', res.statusCode)
    console.log('Response:', body.slice(0, 500))
  })
})

req.on('error', (err) => {
  console.error('Upload error:', err.message)
})

req.write(data)
req.end()
