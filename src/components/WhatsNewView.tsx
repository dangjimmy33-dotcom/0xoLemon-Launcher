import { Sparkles, Calendar, Tag } from 'lucide-react'
import { useLocale } from '../context/LocaleContext'
import CHANGELOG from '../changelog.json'

export function WhatsNewView({ isModal = false }: { isModal?: boolean }) {
  const { t } = useLocale()
  
  return (
    <section className={isModal ? "" : "single-view"} style={{ overflowY: 'auto', height: '100%', padding: isModal ? '0' : '32px 48px', display: 'flex', justifyContent: 'center' }}>
      <div style={{ maxWidth: '800px', width: '100%', margin: '0 auto', display: 'flex', flexDirection: 'column', gap: '32px', padding: isModal ? '24px' : undefined }}>
        {!isModal && (
          <header style={{ 
            marginBottom: '8px', 
            padding: '32px', 
            background: 'linear-gradient(135deg, rgba(77, 164, 255, 0.15) 0%, rgba(0, 0, 0, 0.2) 100%)', 
            borderRadius: '16px',
            border: '1px solid rgba(77, 164, 255, 0.2)',
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            textAlign: 'center',
            boxShadow: '0 8px 32px rgba(0,0,0,0.2)'
          }}>
            <div style={{ background: 'rgba(77, 164, 255, 0.2)', padding: '16px', borderRadius: '50%', marginBottom: '16px' }}>
              <Sparkles size={36} style={{ color: '#4da4ff' }} />
            </div>
            <h2 style={{ fontSize: '2rem', margin: 0, fontWeight: 700, letterSpacing: '-0.02em', color: '#fff' }}>
              {t.whatsNew.title}
            </h2>
            <p style={{ color: 'rgba(255, 255, 255, 0.7)', marginTop: '12px', fontSize: '1.1rem', maxWidth: '500px', lineHeight: 1.5 }}>
              {t.whatsNew.subtitle}
            </p>
          </header>
        )}

        <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
          {CHANGELOG.map((release, index) => (
            <section key={release.version} style={{
              background: 'rgba(255, 255, 255, 0.03)',
              borderRadius: '12px',
              border: '1px solid rgba(255, 255, 255, 0.05)',
              overflow: 'hidden',
              transition: 'transform 0.2s, background 0.2s',
            }} className="changelog-card">
              <header style={{ 
                padding: '20px 24px', 
                borderBottom: '1px solid rgba(255, 255, 255, 0.05)',
                background: index === 0 ? 'rgba(77, 164, 255, 0.08)' : 'rgba(0, 0, 0, 0.2)',
                display: 'flex', 
                justifyContent: 'space-between', 
                alignItems: 'center' 
              }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                  <div style={{ 
                    width: '36px', 
                    height: '36px', 
                    borderRadius: '8px', 
                    background: index === 0 ? 'rgba(77, 164, 255, 0.2)' : 'rgba(255, 255, 255, 0.1)', 
                    display: 'flex', 
                    alignItems: 'center', 
                    justifyContent: 'center' 
                  }}>
                    <Tag size={18} style={{ color: index === 0 ? '#4da4ff' : 'rgba(255,255,255,0.7)' }} />
                  </div>
                  <strong style={{ color: index === 0 ? '#4da4ff' : '#fff', fontSize: '1.2rem', display: 'flex', alignItems: 'center', gap: '10px' }}>
                    {t.whatsNew.version} {release.version}
                    {index === 0 && (
                      <span style={{ fontSize: '0.75rem', background: '#4da4ff', color: '#000', padding: '4px 10px', borderRadius: '12px', fontWeight: 'bold', textTransform: 'uppercase', letterSpacing: '0.5px' }}>
                        {t.whatsNew.latest}
                      </span>
                    )}
                  </strong>
                </div>
                <span style={{ display: 'flex', alignItems: 'center', gap: '6px', color: 'rgba(255, 255, 255, 0.5)', fontSize: '0.95rem', fontWeight: '500' }}>
                  <Calendar size={16} />
                  {release.date}
                </span>
              </header>
              <div style={{ padding: '24px' }}>
                <ul style={{ 
                  margin: 0, 
                  paddingLeft: '24px', 
                  display: 'flex', 
                  flexDirection: 'column', 
                  gap: '12px', 
                  color: 'rgba(255, 255, 255, 0.85)',
                  fontSize: '1.05rem'
                }}>
                  {release.changes.map((change, i) => (
                    <li key={i} style={{ lineHeight: '1.6' }}>
                      {change}
                    </li>
                  ))}
                </ul>
              </div>
            </section>
          ))}
        </div>
      </div>
    </section>
  )
}
