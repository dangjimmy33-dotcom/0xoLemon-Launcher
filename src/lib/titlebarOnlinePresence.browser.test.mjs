import assert from 'node:assert/strict'
import { existsSync } from 'node:fs'
import { readFile, unlink, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import test from 'node:test'

const cssUrl = new URL('../App.css', import.meta.url)
const titlebarUrl = new URL('../components/CustomTitleBar.tsx', import.meta.url)
const [css, titlebar] = await Promise.all([
  readFile(cssUrl, 'utf8'),
  readFile(titlebarUrl, 'utf8'),
])

function findChromium() {
  const candidates = [
    process.env.EDGE_BIN,
    process.env.CHROME_BIN,
    process.env['ProgramFiles(x86)'] && join(process.env['ProgramFiles(x86)'], 'Microsoft', 'Edge', 'Application', 'msedge.exe'),
    process.env.ProgramFiles && join(process.env.ProgramFiles, 'Microsoft', 'Edge', 'Application', 'msedge.exe'),
    process.env.ProgramFiles && join(process.env.ProgramFiles, 'Google', 'Chrome', 'Application', 'chrome.exe'),
  ].filter(Boolean)

  return candidates.find((candidate) => existsSync(candidate)) ?? null
}

test('CustomTitleBar renders the online chip and anchored social state dot accessibly', () => {
  assert.match(
    titlebar,
    /className="titlebar-online-chip is-clickable"[\s\S]*?<span className="titlebar-online-dot" aria-hidden="true" \/>[\s\S]*?\{onlineCount\}/,
    'clickable online count must keep its decorative dot hidden from assistive technology',
  )
  assert.match(
    titlebar,
    /className=\{`titlebar-social-toggle[\s\S]*?<span className="titlebar-social-state-dot" aria-hidden="true" \/>/,
    'the social state dot must remain a child of its toggle so CSS can anchor it locally',
  )
  assert.match(titlebar, /aria-label=\{`Open Social panel, \$\{onlineCount\} users online`\}/)
  assert.match(titlebar, /aria-pressed=\{socialLayerVisible\}/)
})

const chromium = findChromium()

test('computed titlebar presence styles stay compact and do not animate a halo', {
  skip: chromium ? false : 'Microsoft Edge or Google Chrome is required for computed-style verification',
}, async () => {
  const fixturePath = resolve(tmpdir(), `0xolemon-titlebar-presence-${process.pid}-${Date.now()}.html`)
  const safeCss = css.replaceAll('</style', '<\\/style')
  const fixture = `<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <style>${safeCss}</style>
  </head>
  <body style="margin:0;font:400 14px/20px Inter,system-ui,sans-serif">
    <div class="custom-titlebar">
      <div class="titlebar-status-cluster">
        <button id="online" type="button" class="titlebar-online-chip is-clickable" aria-label="Open Social panel, 1 users online">
          <span id="online-dot" class="titlebar-online-dot" aria-hidden="true"></span>
          <span id="count">1</span>
        </button>
        <button id="social" type="button" class="titlebar-social-toggle" aria-label="Show friends panel" aria-pressed="false">
          <svg width="17" height="17" aria-hidden="true"></svg>
          <span id="social-dot" class="titlebar-social-state-dot" aria-hidden="true"></span>
        </button>
      </div>
    </div>
    <script>
      const online = document.getElementById('online')
      const count = document.getElementById('count')
      const onlineDot = document.getElementById('online-dot')
      const social = document.getElementById('social')
      const socialDot = document.getElementById('social-dot')
      const counts = [1, 9, 99, 999].map((value) => {
        count.textContent = String(value)
        online.setAttribute('aria-label', 'Open Social panel, ' + value + ' users online')
        const rect = online.getBoundingClientRect()
        return { value, width: rect.width, height: rect.height }
      })
      online.focus()
      const onlineStyle = getComputedStyle(online)
      const onlineDotStyle = getComputedStyle(onlineDot)
      const socialRect = social.getBoundingClientRect()
      const socialDotRect = socialDot.getBoundingClientRect()
      const payload = {
        counts,
        online: {
          minHeight: onlineStyle.minHeight,
          paddingTop: onlineStyle.paddingTop,
          paddingRight: onlineStyle.paddingRight,
          paddingBottom: onlineStyle.paddingBottom,
          paddingLeft: onlineStyle.paddingLeft,
          fontSize: onlineStyle.fontSize,
          fontWeight: onlineStyle.fontWeight,
          lineHeight: onlineStyle.lineHeight,
          outlineStyle: onlineStyle.outlineStyle,
          outlineWidth: onlineStyle.outlineWidth,
          focusVisible: online.matches(':focus-visible'),
        },
        onlineDot: {
          animationName: onlineDotStyle.animationName,
          boxShadow: onlineDotStyle.boxShadow,
          width: onlineDotStyle.width,
          height: onlineDotStyle.height,
        },
        social: {
          offsetParentId: socialDot.offsetParent && socialDot.offsetParent.id,
          minHeight: getComputedStyle(social).minHeight,
          dotInside:
            socialDotRect.left >= socialRect.left &&
            socialDotRect.top >= socialRect.top &&
            socialDotRect.right <= socialRect.right &&
            socialDotRect.bottom <= socialRect.bottom,
        },
      }
      document.documentElement.setAttribute('data-titlebar-results', btoa(JSON.stringify(payload)))
    </script>
  </body>
</html>`

  await writeFile(fixturePath, fixture, 'utf8')
  try {
    const result = spawnSync(chromium, [
      '--headless=new',
      '--disable-gpu',
      '--disable-extensions',
      '--disable-default-apps',
      '--no-first-run',
      '--dump-dom',
      new URL(`file:///${fixturePath.replaceAll('\\\\', '/')}`).href,
    ], {
      encoding: 'utf8',
      maxBuffer: 8 * 1024 * 1024,
      timeout: 20_000,
      windowsHide: true,
    })

    assert.equal(result.error, undefined, result.error?.message)
    assert.equal(result.status, 0, result.stderr || 'headless browser failed')
    const encoded = result.stdout.match(/data-titlebar-results="([A-Za-z0-9+/=]+)"/)?.[1]
    assert.ok(encoded, 'computed-style payload was not emitted by the browser fixture')
    const computed = JSON.parse(Buffer.from(encoded, 'base64').toString('utf8'))

    assert.deepEqual(computed.online, {
      minHeight: '0px',
      paddingTop: '2px',
      paddingRight: '8px',
      paddingBottom: '2px',
      paddingLeft: '6px',
      fontSize: '11px',
      fontWeight: '600',
      lineHeight: '14px',
      outlineStyle: 'solid',
      outlineWidth: '2px',
      focusVisible: true,
    })
    assert.deepEqual(computed.onlineDot, {
      animationName: 'none',
      boxShadow: 'none',
      width: '6px',
      height: '6px',
    })
    assert.equal(computed.social.offsetParentId, 'social')
    assert.equal(computed.social.minHeight, '28px')
    assert.equal(computed.social.dotInside, true)

    assert.deepEqual(computed.counts.map(({ value }) => value), [1, 9, 99, 999])
    assert.ok(computed.counts.every(({ height }) => height <= 22), `online chip heights are not compact: ${JSON.stringify(computed.counts)}`)
    assert.ok(computed.counts.at(-1).width <= 52, `999 online chip is too wide: ${computed.counts.at(-1).width}px`)
    assert.ok(computed.counts.at(-1).width > computed.counts[0].width, 'online chip must expand only as its count gains digits')
  } finally {
    await unlink(fixturePath).catch(() => {})
  }
})
