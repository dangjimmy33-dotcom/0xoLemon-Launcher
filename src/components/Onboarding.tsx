import { useState } from 'react'
import { AnimatePresence, motion } from 'motion/react'
import { Bell, ChevronLeft, ChevronRight, Gamepad2, Home, Sparkles, X } from 'lucide-react'
import { MOTION } from '../lib/motion'

const steps = [
  {
    icon: Home,
    title: 'Everything starts at Home',
    body: 'Continue recent games, track active work and reach community tools without hunting through menus.',
  },
  {
    icon: Sparkles,
    title: 'A calmer, responsive interface',
    body: 'Motion explains state changes and can follow Windows settings or be reduced at any time.',
  },
  {
    icon: Bell,
    title: 'Notifications you can trust',
    body: 'Only real install, update, cloud-save, storage and launcher events are recorded.',
  },
  {
    icon: Gamepad2,
    title: 'Ready when you are',
    body: 'Your existing Store, Library, downloads and cloud-save configuration stay intact.',
  },
]

export function Onboarding({
  onComplete,
  onEnableWindowsNotifications,
}: {
  onComplete: () => void
  onEnableWindowsNotifications: () => void
}) {
  const [step, setStep] = useState(0)
  const current = steps[step]
  const Icon = current.icon

  function next() {
    if (step === 2) onEnableWindowsNotifications()
    if (step === steps.length - 1) {
      onComplete()
      return
    }
    setStep((value) => value + 1)
  }

  return (
    <div className="onboarding-overlay" role="dialog" aria-modal="true" aria-labelledby="onboarding-title">
      <motion.section className="onboarding-card" initial={{ opacity: 0, scale: 0.97 }} animate={{ opacity: 1, scale: 1 }} transition={MOTION.panel}>
        <button type="button" className="onboarding-skip" onClick={onComplete} aria-label="Skip onboarding"><X size={18} /></button>
        <div className="onboarding-visual">
          <AnimatePresence mode="wait">
            <motion.div
              key={step}
              initial={{ opacity: 0, y: 12, scale: 0.96 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: -10, scale: 0.98 }}
              transition={MOTION.hero}
            >
              <span><Icon size={36} /></span>
              <i />
              <i />
              <i />
            </motion.div>
          </AnimatePresence>
        </div>
        <div className="onboarding-copy">
          <span>Step {step + 1} of {steps.length}</span>
          <h2 id="onboarding-title">{current.title}</h2>
          <p>{current.body}</p>
          {step === 2 ? <small>Windows notifications are optional and work only in installed builds.</small> : null}
        </div>
        <div className="onboarding-dots">
          {steps.map((item, index) => <i key={item.title} className={index === step ? 'is-active' : ''} />)}
        </div>
        <footer>
          <button type="button" className="onboarding-secondary" disabled={step === 0} onClick={() => setStep((value) => value - 1)}>
            <ChevronLeft size={16} /> Back
          </button>
          <button type="button" className="onboarding-primary" onClick={next}>
            {step === steps.length - 1 ? 'Finish' : step === 2 ? 'Enable and continue' : 'Continue'}
            {step < steps.length - 1 ? <ChevronRight size={16} /> : null}
          </button>
        </footer>
      </motion.section>
    </div>
  )
}

