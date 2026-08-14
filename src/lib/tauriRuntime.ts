type TauriWindow = Window & {
  __TAURI__?: unknown
  __TAURI_INTERNALS__?: unknown
}

export function isTauriRuntime(): boolean {
  if (typeof window === 'undefined') return false

  const tauriWindow = window as TauriWindow
  return Boolean(
    tauriWindow.__TAURI__ ||
      tauriWindow.__TAURI_INTERNALS__ ||
      window.location.protocol === 'tauri:' ||
      window.location.protocol === 'asset:' ||
      window.location.hostname === 'tauri.localhost',
  )
}
