interface Env {
  HF_ORIGIN_BASE?: string
  HF_BEARER_TOKEN?: string
}

const SAFE_VERSION = /^v[0-9]+(?:\.[0-9]+){0,3}[A-Za-z0-9_-]*$/
const SAFE_PACK_ID = /^pack-[0-9]{5}$/

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const requestId = crypto.randomUUID()
    try {
      const response = await routeRequest(request, env, requestId)
      ctx.waitUntil(logRequest(request, response.status, requestId))
      return response
    } catch (error) {
      const response = json(
        {
          error: 'update_proxy_error',
          requestId,
          message: error instanceof Error ? error.message : 'Unknown proxy error',
        },
        502,
      )
      ctx.waitUntil(logRequest(request, response.status, requestId))
      return response
    }
  },
} satisfies ExportedHandler<Env>

async function routeRequest(request: Request, env: Env, requestId: string): Promise<Response> {
  if (request.method !== 'GET' && request.method !== 'HEAD') {
    return json({ error: 'method_not_allowed', requestId }, 405, { Allow: 'GET, HEAD' })
  }

  if (!env.HF_ORIGIN_BASE) {
    return json({ error: 'origin_not_configured', requestId }, 503)
  }

  const url = new URL(request.url)
  const originPath = resolveOriginPath(url.pathname)
  if (!originPath) {
    return json({ error: 'not_found', requestId }, 404)
  }

  const originBase = env.HF_ORIGIN_BASE.replace(/\/+$/, '')
  const originUrl = `${originBase}/${originPath}`
  const headers = new Headers()
  copyHeader(request.headers, headers, 'range')
  copyHeader(request.headers, headers, 'if-none-match')
  copyHeader(request.headers, headers, 'if-modified-since')
  headers.set('accept', 'application/octet-stream')
  headers.set('user-agent', 'first-light-launcher-proxy/0.1')
  if (env.HF_BEARER_TOKEN) {
    headers.set('authorization', `Bearer ${env.HF_BEARER_TOKEN}`)
  }

  const upstream = await fetch(originUrl, {
    method: request.method,
    headers,
    redirect: 'follow',
  })

  const responseHeaders = new Headers(upstream.headers)
  responseHeaders.set('x-launcher-request-id', requestId)
  responseHeaders.set('cache-control', cacheControlFor(originPath))
  responseHeaders.delete('server')
  responseHeaders.delete('set-cookie')

  return new Response(upstream.body, {
    status: upstream.status,
    statusText: upstream.statusText,
    headers: responseHeaders,
  })
}

function resolveOriginPath(pathname: string): string | null {
  const parts = pathname.split('/').filter(Boolean)
  if (parts.length === 1 && parts[0] === 'catalog') {
    return 'catalog.json'
  }
  if (parts.length === 2 && parts[0] === 'manifests' && SAFE_VERSION.test(parts[1] ?? '')) {
    return `versions/${parts[1]}/manifest.json`
  }
  if (parts.length === 2 && parts[0] === 'packs' && SAFE_PACK_ID.test(parts[1] ?? '')) {
    return `packs/${parts[1]}.bin`
  }
  return null
}

function cacheControlFor(originPath: string): string {
  if (originPath === 'catalog.json') {
    return 'public, max-age=60'
  }
  if (originPath.startsWith('manifests/')) {
    return 'public, max-age=300, immutable'
  }
  return 'public, max-age=31536000, immutable'
}

function copyHeader(from: Headers, to: Headers, name: string): void {
  const value = from.get(name)
  if (value) {
    to.set(name, value)
  }
}

function json(body: unknown, status: number, extraHeaders?: HeadersInit): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      'content-type': 'application/json; charset=utf-8',
      ...extraHeaders,
    },
  })
}

async function logRequest(request: Request, status: number, requestId: string): Promise<void> {
  console.log(
    JSON.stringify({
      requestId,
      method: request.method,
      path: new URL(request.url).pathname,
      status,
    }),
  )
}
