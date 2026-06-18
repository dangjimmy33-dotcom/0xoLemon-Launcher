import { useEffect, useRef, useState } from 'react'

export function useSmoothNumber(target: number, durationMs = 420) {
  const [value, setValue] = useState(target)
  const valueRef = useRef(target)
  const safeTarget = Number.isFinite(target) ? target : 0

  useEffect(() => {
    const start = valueRef.current
    const end = safeTarget
    if (Math.abs(end - start) < 0.08) {
      valueRef.current = end
      const raf = requestAnimationFrame(() => setValue(end))
      return () => cancelAnimationFrame(raf)
    }

    let raf = 0
    const startedAt = performance.now()
    const tick = (now: number) => {
      const elapsed = Math.min((now - startedAt) / durationMs, 1)
      const eased = 1 - Math.pow(1 - elapsed, 3)
      const next = start + (end - start) * eased
      valueRef.current = next
      setValue(next)
      if (elapsed < 1) {
        raf = requestAnimationFrame(tick)
      } else {
        valueRef.current = end
        setValue(end)
      }
    }
    raf = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(raf)
  }, [durationMs, safeTarget])

  return value
}
