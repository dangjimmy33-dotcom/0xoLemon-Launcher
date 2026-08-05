/**
 * fetchWithRetry — Fetch với exponential backoff tự động retry khi mạng chập chờn.
 *
 * Chỉ retry khi:
 *   - Network error (TypeError: Failed to fetch)
 *   - HTTP 429, 503, 504 (rate limit / server unavailable)
 * Không retry khi:
 *   - HTTP 4xx khác (lỗi client, retry vô ích)
 *   - HTTP 200-399 (thành công)
 */

export interface RetryOptions {
  /** Số lần retry tối đa. Mặc định: 3 */
  maxRetries?: number
  /** Thời gian chờ cơ bản (ms). Mặc định: 1000 */
  baseDelay?: number
  /** Hệ số nhân cho mỗi lần retry. Mặc định: 2 (exponential) */
  backoffFactor?: number
  /** Jitter tối đa (ms) để tránh thundering herd. Mặc định: 500 */
  jitter?: number
  /** Timeout cho mỗi request (ms). Mặc định: 15000 */
  timeout?: number
  /** Callback khi mỗi lần retry */
  onRetry?: (attempt: number, error: Error, delayMs: number) => void
}

const RETRYABLE_STATUS = new Set([429, 500, 502, 503, 504])

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function calcDelay(attempt: number, baseDelay: number, backoffFactor: number, jitter: number): number {
  const exponential = baseDelay * Math.pow(backoffFactor, attempt)
  const noise = Math.random() * jitter
  return Math.min(exponential + noise, 30_000) // cap 30s
}

export async function fetchWithRetry(
  url: string,
  init?: RequestInit,
  options: RetryOptions = {},
): Promise<Response> {
  const {
    maxRetries = 3,
    baseDelay = 1000,
    backoffFactor = 2,
    jitter = 500,
    timeout = 15_000,
    onRetry,
  } = options

  let lastError: Error = new Error('Unknown error')

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    // AbortController per attempt để enforce timeout
    const controller = new AbortController()
    const timer = setTimeout(() => controller.abort(), timeout)

    try {
      const response = await fetch(url, {
        ...init,
        signal: controller.signal,
      })
      clearTimeout(timer)

      // Thành công hoặc lỗi client không cần retry
      if (response.ok || !RETRYABLE_STATUS.has(response.status)) {
        return response
      }

      // HTTP error có thể retry (5xx, 429)
      lastError = new Error(`HTTP ${response.status}: ${response.statusText}`)
    } catch (err) {
      clearTimeout(timer)
      // Network error hoặc timeout
      if (err instanceof Error) {
        lastError = err
      } else {
        lastError = new Error(String(err))
      }
    }

    // Không retry ở lần cuối
    if (attempt === maxRetries) break

    const delayMs = calcDelay(attempt, baseDelay, backoffFactor, jitter)
    onRetry?.(attempt + 1, lastError, delayMs)
    console.warn(
      `[fetchWithRetry] Attempt ${attempt + 1}/${maxRetries} failed: ${lastError.message}. Retrying in ${Math.round(delayMs)}ms...`,
    )
    await sleep(delayMs)
  }

  throw lastError
}
