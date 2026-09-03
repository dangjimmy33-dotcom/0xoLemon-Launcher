import { isTauriRuntime } from './lib/tauriRuntime'

function renderBootstrapFailure(error: unknown) {
  console.error('0xoLemon bootstrap failed:', error)

  const root = document.getElementById('root')
  if (!root) return

  const detail = error instanceof Error ? error.message : String(error)
  root.innerHTML = `
    <main style="min-height:100vh;display:grid;place-items:center;background:#080d12;color:#f5f7fa;padding:32px;box-sizing:border-box;font-family:Inter,Segoe UI,sans-serif">
      <section style="width:min(680px,100%);border:1px solid #34404b;background:#0e151c;padding:24px;border-radius:8px">
        <h1 style="font-size:20px;margin:0 0 12px">0xoLemon could not finish starting</h1>
        <p style="line-height:1.6;color:#b9c3cc;margin:0 0 14px">The correct application surface could not be loaded. Close this window and try again.</p>
        <pre style="white-space:pre-wrap;word-break:break-word;color:#ffb4a9;background:#090e13;padding:12px;border-radius:6px;margin:0"></pre>
      </section>
    </main>`
  const pre = root.querySelector('pre')
  if (pre) pre.textContent = detail
}

async function bootstrap() {
  const fixture = import.meta.env.DEV
    ? new URLSearchParams(window.location.search).get('fixture')
    : null

  if (isTauriRuntime() || fixture) {
    const desktop = await import('./desktop/bootstrap')
    await desktop.bootstrapDesktop()
    return
  }

  const web = await import('./web/bootstrap')
  await web.bootstrapWeb()
}

void bootstrap().catch(renderBootstrapFailure)
