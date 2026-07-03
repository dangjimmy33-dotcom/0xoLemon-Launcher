import { createPortal } from 'react-dom'
import { X, Sparkles } from 'lucide-react'
import { WhatsNewView } from './WhatsNewView'
import { useLocale } from '../context/LocaleContext'

export function ChangelogModal({
  onClose,
}: {
  onClose: () => void
}) {
  const { t } = useLocale()

  return createPortal(
    <div
      className="modal-backdrop"
      onClick={onClose}
      style={{
        position: 'fixed',
        inset: 0,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 1000,
      }}
    >
      <div 
        className="modal-content" 
        onClick={(e) => e.stopPropagation()}
        style={{ 
          maxWidth: '850px', 
          width: '90%', 
          height: '80vh', 
          display: 'flex', 
          flexDirection: 'column',
          padding: 0,
          overflow: 'hidden',
          margin: 0,
        }}
      >
        <header style={{ 
          display: 'flex', 
          justifyContent: 'space-between', 
          alignItems: 'center', 
          padding: '20px 24px',
          borderBottom: '1px solid rgba(255, 255, 255, 0.05)',
          background: 'rgba(0,0,0,0.2)'
        }}>
          <h2 style={{ margin: 0, fontSize: '1.2rem', display: 'flex', alignItems: 'center', gap: '8px' }}>
            <Sparkles size={20} style={{ color: '#4da4ff' }} />
            {t.whatsNew.title}
          </h2>
          <button 
            type="button" 
            className="icon-button" 
            onClick={onClose}
            style={{ background: 'transparent', border: 'none', color: '#fff', cursor: 'pointer' }}
          >
            <X size={24} />
          </button>
        </header>
        
        <div style={{ flex: 1, overflowY: 'auto', position: 'relative' }}>
          {/* Reuse the view but we might need to tweak its internal padding if it has too much margin */}
          <WhatsNewView isModal />
        </div>
        
        <footer style={{ 
          padding: '16px 24px', 
          borderTop: '1px solid rgba(255, 255, 255, 0.05)',
          display: 'flex',
          justifyContent: 'flex-end',
          background: 'rgba(0,0,0,0.2)'
        }}>
          <button 
            type="button" 
            className="primary-button" 
            onClick={onClose}
            style={{
              padding: '8px 24px',
              borderRadius: '6px',
              background: '#4da4ff',
              color: '#000',
              fontWeight: 'bold',
              border: 'none',
              cursor: 'pointer'
            }}
          >
            {t.whatsNew.close}
          </button>
        </footer>
      </div>
    </div>,
    document.body
  )
}
