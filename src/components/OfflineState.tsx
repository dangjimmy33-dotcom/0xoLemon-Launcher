import { CloudOff } from 'lucide-react'
import { useLocale } from '../context/LocaleContext'
export function OfflineState() {
  const { t } = useLocale()

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100%',
        width: '100%',
        color: 'var(--text-color)',
        padding: '24px',
        textAlign: 'center',
        opacity: 0.8
      }}
    >
      <CloudOff size={64} style={{ marginBottom: '16px', color: '#ff6b6b' }} />
      <h2 style={{ fontSize: '24px', fontWeight: 600, marginBottom: '8px' }}>
        {t.offline.title || 'Oh no, connection error!'}
      </h2>
      <p style={{ fontSize: '15px', color: 'var(--text-muted)' }}>
        {t.offline.description || 'Please check your internet connection and retry later.'}
      </p>
    </div>
  )
}
