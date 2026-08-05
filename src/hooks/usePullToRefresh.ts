import { useEffect, useRef, useCallback } from 'react'

interface PullToRefreshOptions {
  /** Ngưỡng deltaY tích lũy (px) để kích hoạt refresh. Mặc định: 150 */
  threshold?: number
  /** Callback khi trigger refresh */
  onRefresh?: () => void
  /** Callback cập nhật progress (0-1) để animate indicator */
  onProgress?: (progress: number) => void
}

/**
 * usePullToRefresh — Pull-to-refresh chuẩn mobile:
 * Khi ở đầu trang, kéo xuống (touch) vượt ngưỡng và thả ra → reload
 */
export function usePullToRefresh(
  containerRef: React.RefObject<HTMLElement | null>,
  options: PullToRefreshOptions = {},
) {
  const {
    threshold = 150,
    onRefresh = () => location.reload(),
    onProgress,
  } = options

  const accumulated = useRef(0)
  const refreshing = useRef(false)
  const startY = useRef<number | null>(null)
  const isPulling = useRef(false)
  const wheelTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const clear = useCallback(() => {
    accumulated.current = 0
    isPulling.current = false
    refreshing.current = false
    if (wheelTimer.current) {
      clearTimeout(wheelTimer.current)
      wheelTimer.current = null
    }
    onProgress?.(0)
  }, [onProgress])

  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const safeEl: HTMLElement = el

    function getScrollTop() {
      return safeEl.scrollTop ?? document.documentElement.scrollTop ?? 0
    }

    function handleWheel(e: WheelEvent) {
      if (refreshing.current) return
      const scrollTop = getScrollTop()

      // Chỉ bắt khi ở đầu trang (scrollTop ≈ 0) VÀ cuộn lên (deltaY < 0 tương đương kéo trang xuống)
      if (scrollTop > 5 || e.deltaY >= 0) {
        clear()
        return
      }

      // Wheel event: chỉ tích lũy khi cuộn mạnh và liên tục
      const absDelta = Math.abs(e.deltaY)
      if (absDelta < 15) return

      accumulated.current += absDelta
      const progress = Math.min(accumulated.current / threshold, 1)
      onProgress?.(progress)

      if (wheelTimer.current) {
        clearTimeout(wheelTimer.current)
      }

      wheelTimer.current = setTimeout(() => {
        if (accumulated.current >= threshold && !refreshing.current) {
          refreshing.current = true
          onProgress?.(1)
          setTimeout(() => onRefresh(), 200)
        } else {
          clear()
        }
      }, 300)
    }

    function handleScroll() {
      if (getScrollTop() > 5) clear()
    }

    // Touch support (mobile pull-to-refresh)
    function handleTouchStart(e: TouchEvent) {
      if (refreshing.current) return
      const scrollTop = getScrollTop()
      if (scrollTop <= 5) {
        startY.current = e.touches[0].clientY
        isPulling.current = false
        accumulated.current = 0
      }
    }

    function handleTouchMove(e: TouchEvent) {
      if (refreshing.current || startY.current === null) return
      const scrollTop = getScrollTop()
      if (scrollTop > 5) {
        clear()
        return
      }

      const currentY = e.touches[0].clientY
      const deltaY = currentY - startY.current

      // Chỉ tích lũy khi kéo XUỐNG (deltaY > 0)
      if (deltaY <= 0) {
        if (isPulling.current) clear()
        return
      }

      isPulling.current = true

      // Resistance effect: làm chậm dần khi kéo xa
      const resistance = Math.max(0.3, 1 - deltaY / (threshold * 2.5))
      accumulated.current = deltaY * resistance

      const progress = Math.min(accumulated.current / threshold, 1)
      onProgress?.(progress)

      // Ngăn browser default pull-to-refresh / overscroll
      if (e.cancelable) {
        e.preventDefault()
      }
    }

    function handleTouchEnd() {
      if (!isPulling.current) return
      
      if (accumulated.current >= threshold && !refreshing.current) {
        refreshing.current = true
        onProgress?.(1)
        setTimeout(() => onRefresh(), 200)
      } else {
        clear()
      }
      startY.current = null
      isPulling.current = false
    }

    safeEl.addEventListener('wheel', handleWheel, { passive: true })
    safeEl.addEventListener('scroll', handleScroll, { passive: true })
    safeEl.addEventListener('touchstart', handleTouchStart, { passive: true })
    safeEl.addEventListener('touchmove', handleTouchMove, { passive: false }) // passive=false to enable preventDefault
    safeEl.addEventListener('touchend', handleTouchEnd, { passive: true })
    safeEl.addEventListener('touchcancel', handleTouchEnd, { passive: true })

    return () => {
      safeEl.removeEventListener('wheel', handleWheel)
      safeEl.removeEventListener('scroll', handleScroll)
      safeEl.removeEventListener('touchstart', handleTouchStart)
      safeEl.removeEventListener('touchmove', handleTouchMove)
      safeEl.removeEventListener('touchend', handleTouchEnd)
      safeEl.removeEventListener('touchcancel', handleTouchEnd)
      if (wheelTimer.current) clearTimeout(wheelTimer.current)
    }
  }, [containerRef, threshold, onRefresh, onProgress, clear])
}
