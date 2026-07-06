import { motion } from 'motion/react'

export function NoInternetView({ tabName }: { tabName?: string }) {
  return (
    <motion.div
      className="no-internet-view"
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3 }}
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100%',
        width: '100%',
        gap: 20,
        padding: '40px 24px',
        textAlign: 'center',
        userSelect: 'none',
      }}
    >
      {/* SVG signal/wifi off icon */}
      <svg
        width="96"
        height="96"
        viewBox="0 0 96 96"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        aria-hidden="true"
        style={{ opacity: 0.25 }}
      >
        <circle cx="48" cy="48" r="46" stroke="currentColor" strokeWidth="3" />
        {/* Wifi arcs */}
        <path
          d="M20 46c7.6-7.6 18-12 28-12s20.4 4.4 28 12"
          stroke="currentColor"
          strokeWidth="5"
          strokeLinecap="round"
          opacity="0.5"
        />
        <path
          d="M30 56c5-5 11.4-8 18-8s13 3 18 8"
          stroke="currentColor"
          strokeWidth="5"
          strokeLinecap="round"
          opacity="0.7"
        />
        <circle cx="48" cy="65" r="4" fill="currentColor" />
        {/* Slash */}
        <line x1="20" y1="20" x2="76" y2="76" stroke="currentColor" strokeWidth="5" strokeLinecap="round" />
      </svg>

      <div style={{ maxWidth: 340 }}>
        <h2 style={{
          fontSize: '1.3rem',
          fontWeight: 700,
          marginBottom: 10,
          color: 'var(--text-color, #e8eaf0)',
          letterSpacing: '-0.01em',
        }}>
          No internet connection
        </h2>
        <p style={{
          fontSize: '0.9rem',
          color: 'var(--text-muted, #6b7280)',
          lineHeight: 1.6,
        }}>
          {tabName
            ? <><strong style={{ color: 'var(--text-color, #e8eaf0)' }}>{tabName}</strong> requires an internet connection.</>
            : 'This section requires an internet connection.'}
          {' '}Connect to a network and it will refresh automatically.
        </p>
      </div>
    </motion.div>
  )
}
