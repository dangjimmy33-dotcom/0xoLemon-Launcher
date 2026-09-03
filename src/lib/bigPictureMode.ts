import { isTauriRuntime } from './tauriRuntime'

export type BigPictureFullscreenMode = 'browser' | 'native' | 'borderless-fallback' | 'failed'

export type BigPictureFullscreenSession = {
  supported: boolean
  mode: BigPictureFullscreenMode
  previousFullscreen: boolean
  previousMaximized: boolean
  previousAlwaysOnTop: boolean
  previousResizable: boolean
  previousPosition?: { x: number; y: number }
  previousInnerSize?: { width: number; height: number }
  /** Internal cleanup for focus-loss repair while Big Picture is active. */
  disposeMaintenance?: () => void
}

const settle = (ms: number) => new Promise<void>((resolve) => window.setTimeout(resolve, ms))

function nearlyFillsMonitor(
  size: { width: number; height: number },
  monitorSize: { width: number; height: number },
) {
  return size.width >= monitorSize.width - 2 && size.height >= monitorSize.height - 2
}

/**
 * Align the WebView *client area* to the physical monitor, not just the outer
 * window frame. On Windows an undecorated/resizable window can still have an
 * invisible resize frame, so outer (0,0) can leave the WebView several pixels
 * inset from the monitor edge. Tauri setSize() targets the INNER size while
 * setPosition() targets the OUTER position, so we measure the client inset and
 * compensate it explicitly.
 */
async function coverMonitorWithClientArea(
  appWindow: Awaited<ReturnType<typeof import('@tauri-apps/api/window')['getCurrentWindow']>>,
  monitor: { position: { x: number; y: number }; size: { width: number; height: number } },
  PhysicalPosition: typeof import('@tauri-apps/api/window')['PhysicalPosition'],
  PhysicalSize: typeof import('@tauri-apps/api/window')['PhysicalSize'],
) {
  if (await appWindow.isFullscreen()) await appWindow.setFullscreen(false)
  if (await appWindow.isMaximized()) await appWindow.unmaximize()
  if (await appWindow.isResizable()) await appWindow.setResizable(false)

  await appWindow.setSize(new PhysicalSize(monitor.size.width, monitor.size.height))
  await appWindow.setPosition(new PhysicalPosition(monitor.position.x, monitor.position.y))
  await appWindow.setFocus()
  await settle(70)

  // Two passes are intentional: Windows/WebView2 can update the invisible frame
  // metrics one tick after resizable/maximized state changes, especially on DPI
  // scaled displays.
  for (let pass = 0; pass < 2; pass += 1) {
    const [innerPosition, innerSize, outerPosition] = await Promise.all([
      appWindow.innerPosition(),
      appWindow.innerSize(),
      appWindow.outerPosition(),
    ])

    const deltaX = monitor.position.x - innerPosition.x
    const deltaY = monitor.position.y - innerPosition.y
    const needsPositionCorrection = Math.abs(deltaX) > 1 || Math.abs(deltaY) > 1
    const needsSizeCorrection = !nearlyFillsMonitor(innerSize, monitor.size)

    if (!needsPositionCorrection && !needsSizeCorrection) break

    if (needsSizeCorrection) {
      await appWindow.setSize(new PhysicalSize(monitor.size.width, monitor.size.height))
    }
    if (needsPositionCorrection) {
      await appWindow.setPosition(new PhysicalPosition(outerPosition.x + deltaX, outerPosition.y + deltaY))
    }
    await settle(45)
  }
}


/**
 * Re-assert a borderless Windows Big Picture window after shell overlays (for
 * example Snipping Tool / Print Screen) temporarily steal focus. Windows can
 * promote the taskbar above a transparent always-on-top window even though the
 * flag itself still reports true. Toggling the z-order flag on focus return and
 * re-aligning the WebView client area repairs that state without polling.
 */
async function repairBorderlessBigPicture(
  appWindow: Awaited<ReturnType<typeof import('@tauri-apps/api/window')['getCurrentWindow']>>,
  currentMonitor: typeof import('@tauri-apps/api/window')['currentMonitor'],
  PhysicalPosition: typeof import('@tauri-apps/api/window')['PhysicalPosition'],
  PhysicalSize: typeof import('@tauri-apps/api/window')['PhysicalSize'],
) {
  const monitor = await currentMonitor()
  if (!monitor) return

  await coverMonitorWithClientArea(appWindow, monitor, PhysicalPosition, PhysicalSize)

  // Force a fresh HWND_TOPMOST z-order transition. Merely calling true again is
  // not enough on some Windows 11 shell transitions because the state bit can
  // remain true while the taskbar was temporarily promoted above the window.
  if (await appWindow.isAlwaysOnTop()) {
    await appWindow.setAlwaysOnTop(false)
    await settle(12)
  }
  await appWindow.setAlwaysOnTop(true)
  await appWindow.setFocus()

  // Let React/ResizeObserver see the final client rect after the native repair.
  window.requestAnimationFrame(() => window.dispatchEvent(new Event('resize')))
}

async function installBigPictureMaintenance(
  appWindow: Awaited<ReturnType<typeof import('@tauri-apps/api/window')['getCurrentWindow']>>,
  session: BigPictureFullscreenSession,
  currentMonitor: typeof import('@tauri-apps/api/window')['currentMonitor'],
  PhysicalPosition: typeof import('@tauri-apps/api/window')['PhysicalPosition'],
  PhysicalSize: typeof import('@tauri-apps/api/window')['PhysicalSize'],
) {
  if (session.mode !== 'borderless-fallback') return

  let lostFocus = false
  let repairing = false
  let disposed = false
  let repairTimer: number | null = null

  const scheduleRepair = () => {
    if (disposed || repairing) return
    if (repairTimer !== null) window.clearTimeout(repairTimer)
    repairTimer = window.setTimeout(() => {
      repairTimer = null
      if (disposed || repairing) return
      repairing = true
      void repairBorderlessBigPicture(appWindow, currentMonitor, PhysicalPosition, PhysicalSize)
        .catch((error) => {
          if (import.meta.env.DEV) console.warn('[BigPicture] Focus-return repair failed', error)
        })
        .finally(() => {
          repairing = false
        })
    }, 90)
  }

  const unlistenFocus = await appWindow.onFocusChanged(({ payload: focused }) => {
    if (!focused) {
      lostFocus = true
      return
    }
    if (!lostFocus) return
    lostFocus = false
    scheduleRepair()
  })

  const handleVisibility = () => {
    if (document.visibilityState === 'visible' && lostFocus) scheduleRepair()
  }
  document.addEventListener('visibilitychange', handleVisibility)

  session.disposeMaintenance = () => {
    disposed = true
    if (repairTimer !== null) window.clearTimeout(repairTimer)
    unlistenFocus()
    document.removeEventListener('visibilitychange', handleVisibility)
  }
}

/** Enter native Big Picture fullscreen, with a deterministic Windows fallback. */
export async function enterNativeBigPictureFullscreen(): Promise<BigPictureFullscreenSession> {
  if (!isTauriRuntime()) {
    return {
      supported: false,
      mode: 'browser',
      previousFullscreen: false,
      previousMaximized: false,
      previousAlwaysOnTop: false,
      previousResizable: true,
    }
  }

  try {
    const { PhysicalPosition, PhysicalSize, currentMonitor, getCurrentWindow } = await import('@tauri-apps/api/window')
    const appWindow = getCurrentWindow()
    const [
      previousFullscreen,
      previousMaximized,
      previousAlwaysOnTop,
      previousResizable,
      previousPosition,
      previousInnerSize,
    ] = await Promise.all([
      appWindow.isFullscreen(),
      appWindow.isMaximized(),
      appWindow.isAlwaysOnTop(),
      appWindow.isResizable(),
      appWindow.outerPosition(),
      appWindow.innerSize(),
    ])

    const session: BigPictureFullscreenSession = {
      supported: true,
      mode: 'native',
      previousFullscreen,
      previousMaximized,
      previousAlwaysOnTop,
      previousResizable,
      previousPosition: { x: previousPosition.x, y: previousPosition.y },
      previousInnerSize: { width: previousInnerSize.width, height: previousInnerSize.height },
    }

    const monitor = await currentMonitor()
    const isWindows = /Windows/i.test(navigator.userAgent)

    if (!previousAlwaysOnTop) await appWindow.setAlwaysOnTop(true)

    // Transparent Tauri windows on Windows have documented fullscreen/taskbar
    // layering quirks. Use a monitor-sized borderless path there, but align the
    // *inner* WebView surface so no invisible resize frame leaks along top/left.
    if (isWindows && monitor) {
      await coverMonitorWithClientArea(appWindow, monitor, PhysicalPosition, PhysicalSize)
      session.mode = 'borderless-fallback'
      await installBigPictureMaintenance(appWindow, session, currentMonitor, PhysicalPosition, PhysicalSize)
      return session
    }

    if (!previousFullscreen) await appWindow.setFullscreen(true)
    await appWindow.setFocus()
    await settle(120)

    const [fullscreenNow, innerSize] = await Promise.all([appWindow.isFullscreen(), appWindow.innerSize()])
    if (monitor && (!fullscreenNow || !nearlyFillsMonitor(innerSize, monitor.size))) {
      await coverMonitorWithClientArea(appWindow, monitor, PhysicalPosition, PhysicalSize)
      await appWindow.setAlwaysOnTop(true)
      session.mode = 'borderless-fallback'
      await installBigPictureMaintenance(appWindow, session, currentMonitor, PhysicalPosition, PhysicalSize)
    }

    return session
  } catch (error) {
    if (import.meta.env.DEV) console.warn('[BigPicture] Native fullscreen enter failed', error)
    return {
      supported: false,
      mode: 'failed',
      previousFullscreen: false,
      previousMaximized: false,
      previousAlwaysOnTop: false,
      previousResizable: true,
    }
  }
}

/** Restore the exact window state that existed before Big Picture opened. */
export async function restoreNativeBigPictureFullscreen(
  session: BigPictureFullscreenSession | null,
): Promise<void> {
  if (!session?.supported || !isTauriRuntime()) return

  try {
    session.disposeMaintenance?.()
    session.disposeMaintenance = undefined

    const { PhysicalPosition, PhysicalSize, getCurrentWindow } = await import('@tauri-apps/api/window')
    const appWindow = getCurrentWindow()

    if (session.mode === 'borderless-fallback') {
      if (await appWindow.isFullscreen()) await appWindow.setFullscreen(false)
      if (await appWindow.isMaximized()) await appWindow.unmaximize()

      if (session.previousPosition) {
        await appWindow.setPosition(new PhysicalPosition(session.previousPosition.x, session.previousPosition.y))
      }
      // setSize() restores INNER size, so snapshot/restore innerSize rather than
      // feeding an outerSize back into an inner-size API.
      if (session.previousInnerSize) {
        await appWindow.setSize(new PhysicalSize(session.previousInnerSize.width, session.previousInnerSize.height))
      }

      if ((await appWindow.isResizable()) !== session.previousResizable) {
        await appWindow.setResizable(session.previousResizable)
      }
      if (session.previousMaximized) await appWindow.maximize()
      if (session.previousFullscreen) await appWindow.setFullscreen(true)
    } else {
      const currentFullscreen = await appWindow.isFullscreen()
      if (currentFullscreen !== session.previousFullscreen) {
        await appWindow.setFullscreen(session.previousFullscreen)
      }
      if ((await appWindow.isResizable()) !== session.previousResizable) {
        await appWindow.setResizable(session.previousResizable)
      }
    }

    const currentAlwaysOnTop = await appWindow.isAlwaysOnTop()
    if (currentAlwaysOnTop !== session.previousAlwaysOnTop) {
      await appWindow.setAlwaysOnTop(session.previousAlwaysOnTop)
    }
    await appWindow.setFocus()
  } catch (error) {
    if (import.meta.env.DEV) console.warn('[BigPicture] Native fullscreen restore failed', error)
  }
}
