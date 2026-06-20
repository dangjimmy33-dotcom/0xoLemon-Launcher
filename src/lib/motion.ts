export const MOTION = {
  micro: { duration: 0.14, ease: [0.2, 0, 0, 1] as const },
  panel: { type: 'spring' as const, stiffness: 380, damping: 34, mass: 0.8 },
  hero: { type: 'spring' as const, stiffness: 240, damping: 30, mass: 1 },
}

