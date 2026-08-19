export type BigPictureFullscreenSession = {
  supported: boolean
  previousFullscreen: boolean
}

function hasTauriRuntime() {
  return typeof window !== 'undefined' && Boolean((window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__)
}

/**
 * Snapshot the window's native fullscreen state and enter fullscreen for Big Picture.
 * Browser/dev preview is intentionally a no-op so the same UI can still be tested there.
 */
export async function enterNativeBigPictureFullscreen(): Promise<BigPictureFullscreenSession> {
  if (!hasTauriRuntime()) {
    return { supported: false, previousFullscreen: false }
  }

  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    const appWindow = getCurrentWindow()
    const previousFullscreen = await appWindow.isFullscreen()
    if (!previousFullscreen) {
      await appWindow.setFullscreen(true)
    }
    return { supported: true, previousFullscreen }
  } catch (error) {
    if (import.meta.env.DEV) console.warn('[BigPicture] Native fullscreen enter failed', error)
    return { supported: false, previousFullscreen: false }
  }
}

/** Restore the exact fullscreen state that existed before Big Picture opened. */
export async function restoreNativeBigPictureFullscreen(
  session: BigPictureFullscreenSession | null,
): Promise<void> {
  if (!session?.supported || !hasTauriRuntime()) return

  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    const appWindow = getCurrentWindow()
    const { previousFullscreen } = session
    const currentFullscreen = await appWindow.isFullscreen()
    if (currentFullscreen !== previousFullscreen) {
      await appWindow.setFullscreen(previousFullscreen)
    }
  } catch (error) {
    if (import.meta.env.DEV) console.warn('[BigPicture] Native fullscreen restore failed', error)
  }
}
