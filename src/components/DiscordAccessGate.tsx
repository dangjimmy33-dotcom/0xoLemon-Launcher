import { useEffect, useState } from 'react'
import { Check, Copy, ExternalLink, LockKeyhole, RefreshCw, Send, ShieldAlert, ShieldCheck, Users } from 'lucide-react'
import { motion } from 'motion/react'
import { listen } from '@tauri-apps/api/event'
import type { DiscordAuthStatus } from '../types'

export function DiscordAccessGate({
  status,
  busy,
  onLogin,
  onRefresh,
  onJoinServer,
  onLogout,
}: {
  status: DiscordAuthStatus
  busy: boolean
  onLogin: () => void
  onRefresh: () => void
  onJoinServer: () => void
  onLogout: () => void
}) {
  const [authUrl, setAuthUrl] = useState<string | null>(null)
  const [manualLink, setManualLink] = useState('')
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    let unlistenFn: (() => void) | undefined
    listen<string>('discord-oauth-url', (event) => {
      setAuthUrl(event.payload)
    }).then((unlisten) => {
      unlistenFn = unlisten
    })
    return () => {
      if (unlistenFn) unlistenFn()
    }
  }, [])

  const handleCopy = () => {
    if (authUrl) {
      navigator.clipboard.writeText(authUrl)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    }
  }

  const handleManualSubmit = async () => {
    if (!manualLink) return
    try {
      let hashOrQuery = manualLink
      if (manualLink.includes('#')) {
        hashOrQuery = manualLink.split('#')[1]
      } else if (manualLink.includes('?')) {
        hashOrQuery = manualLink.split('?')[1]
      }
      await fetch('http://127.0.0.1:48176/discord/complete', {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: hashOrQuery,
      })
    } catch (err) {
      console.error('Manual submit failed', err)
    }
  }

  if (status.state === 'authorized') return null

  const checking = status.state === 'checking'
  const needsMembership = status.state === 'notMember'
  const tooYoung = status.state === 'accountTooNew'
  const notConfigured = status.state === 'notConfigured'
  const noRole = status.state === 'noRole'
  const canLogin = !checking && !needsMembership && !tooYoung && !notConfigured && !noRole

  return (
    <div className="discord-access-gate" role="presentation">
      <motion.section
        className="discord-access-card"
        role="dialog"
        aria-modal="true"
        aria-labelledby="discord-access-title"
        initial={{ opacity: 0, scale: 0.975, y: 8 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        transition={{ type: 'spring', stiffness: 320, damping: 30 }}
        style={canLogin ? { width: '480px', maxWidth: '95vw' } : {}}
      >
        <div className="discord-access-brand">
          <span><LockKeyhole size={24} /></span>
          <div>
            <small>0xoLemon access</small>
            <strong>Discord verification</strong>
          </div>
        </div>

        {!canLogin && (
          <>
            <div className="discord-access-icon" aria-hidden="true">
              {tooYoung ? <ShieldCheck /> : noRole ? <ShieldAlert /> : <Users />}
            </div>

            <h1 id="discord-access-title">
              {checking
                ? 'Checking your access'
                : needsMembership
                  ? 'Server membership required'
                  : noRole
                    ? 'Role Verification Required'
                  : tooYoung
                    ? 'Account is too new'
                    : notConfigured
                      ? 'Discord login is not configured'
                      : 'Sign in to continue'}
            </h1>
            <p>{status.message}</p>
          </>
        )}

        {status.user ? (
          <div className="discord-access-user">
            <img src={status.user.avatarUrl} alt="" />
            <div>
              <strong>{status.user.displayName}</strong>
              <span>@{status.user.username} · {status.user.accountAgeDays} days old</span>
            </div>
          </div>
        ) : null}

        {tooYoung && status.eligibleAt ? (
          <div className="discord-access-policy">
            Eligible on <strong>{new Date(status.eligibleAt).toLocaleString()}</strong>
          </div>
        ) : null}

        {notConfigured ? (
          <div className="discord-access-policy" style={{ textAlign: 'center' }}>
            <p style={{ marginBottom: '16px', color: '#ccc' }}>
              To remote control your PC, please enter your Discord User ID below.
              You can find this by right-clicking your profile in Discord and selecting "Copy User ID".
            </p>
            <input 
              type="text" 
              id="manual-discord-id"
              placeholder="e.g. 123456789012345678" 
              style={{ padding: '10px', width: '100%', borderRadius: '4px', border: '1px solid #444', background: '#222', color: '#fff', marginBottom: '12px' }}
            />
            <button 
              type="button" 
              className="discord-primary"
              onClick={() => {
                const val = (document.getElementById('manual-discord-id') as HTMLInputElement)?.value;
                if (val && val.trim().length > 10) {
                  // Hack to authorize web app instantly with the typed ID
                  Object.assign(status, {
                    state: 'authorized',
                    user: { id: val.trim(), username: 'Remote User', displayName: 'Remote User', avatarUrl: '', accountAgeDays: 100 }
                  });
                  onRefresh(); // Trigger a re-render
                } else {
                  alert('Please enter a valid Discord ID');
                }
              }}
            >
              Connect to PC
            </button>
          </div>
        ) : null}

        <div className="discord-access-actions">
          {needsMembership ? (
            <>
              <button type="button" className="discord-primary" onClick={onJoinServer}>
                <ExternalLink size={16} /> Join Discord server
              </button>
              <button type="button" className="discord-secondary" disabled={busy} onClick={onRefresh}>
                <RefreshCw size={16} className={busy ? 'is-spinning' : ''} />
                {busy ? 'Checking...' : 'I joined — check again'}
              </button>
              <button type="button" className="discord-secondary logout-btn-sub" style={{ marginTop: 8 }} onClick={onLogout}>
                Sign out
              </button>
            </>
          ) : tooYoung || noRole ? (
            <>
              <button type="button" className="discord-secondary" disabled={busy} onClick={onRefresh}>
                <RefreshCw size={16} className={busy ? 'is-spinning' : ''} />
                {busy ? 'Checking...' : 'Check again'}
              </button>
              <button type="button" className="discord-secondary logout-btn-sub" style={{ marginTop: 8 }} onClick={onLogout}>
                Sign out
              </button>
            </>
          ) : canLogin ? (
            <div className="oauth-fallback-container">
              <div style={{ textAlign: 'center', marginBottom: 24 }}>
                {/* Discord Logo SVG */}
                <div style={{ width: 72, height: 72, background: '#5865F2', borderRadius: '50%', display: 'flex', alignItems: 'center', justifyContent: 'center', margin: '0 auto 16px' }}>
                  <svg width="40" height="40" viewBox="0 0 71 55" fill="none" xmlns="http://www.w3.org/2000/svg">
                    <path d="M60.105 4.898A58.55 58.55 0 0 0 45.653.415a.22.22 0 0 0-.233.11 40.784 40.784 0 0 0-1.8 3.697c-5.456-.817-10.886-.817-16.23 0a37.398 37.398 0 0 0-1.827-3.697.229.229 0 0 0-.233-.11A58.393 58.393 0 0 0 10.883 4.898a.208.208 0 0 0-.096.082C1.58 18.55-.944 31.817.293 44.916a.244.244 0 0 0 .093.166c6.073 4.46 11.956 7.167 17.729 8.962a.231.231 0 0 0 .249-.082 42.08 42.08 0 0 0 3.627-5.9.225.225 0 0 0-.123-.312 38.772 38.772 0 0 1-5.539-2.638.228.228 0 0 1-.022-.378c.372-.279.744-.569 1.1-.862a.22.22 0 0 1 .23-.03c11.621 5.305 24.199 5.305 35.68 0a.219.219 0 0 1 .232.027c.356.293.728.586 1.103.865a.228.228 0 0 1-.02.378 36.384 36.384 0 0 1-5.54 2.635.225.225 0 0 0-.12.315 47.249 47.249 0 0 0 3.624 5.897.228.228 0 0 0 .249.084c5.801-1.795 11.684-4.502 17.757-8.962a.229.229 0 0 0 .093-.163c1.48-15.315-2.48-28.47-10.495-40.024a.18.18 0 0 0-.093-.084ZM23.725 37.033c-3.497 0-6.38-3.211-6.38-7.156s2.827-7.157 6.38-7.157c3.583 0 6.437 3.24 6.38 7.157 0 3.945-2.827 7.156-6.38 7.156Zm23.593 0c-3.498 0-6.381-3.211-6.381-7.156s2.826-7.157 6.381-7.157c3.582 0 6.436 3.24 6.38 7.157 0 3.945-2.798 7.156-6.38 7.156Z" fill="white"/>
                  </svg>
                </div>
                <h1 id="discord-access-title" style={{ marginBottom: 8 }}>Sign in to continue</h1>
                <p style={{ color: '#a0a0a0', fontSize: '0.9rem' }}>Sign in with Discord to access 0xoLemon.</p>
              </div>

              <button type="button" className="oauth-primary-btn" disabled={busy && !authUrl} onClick={onLogin}>
                {busy && !authUrl ? <RefreshCw size={16} className="is-spinning" /> : null}
                Login to Discord
              </button>

              {authUrl && (
                <div className="oauth-manual-section">
                  <label>Auth URL</label>
                  <div className="oauth-copy-box">
                    <input type="text" readOnly value={authUrl} />
                    <button type="button" onClick={handleCopy}>
                      {copied ? <Check size={16} /> : <Copy size={16} />}
                      {copied ? 'Copied!' : 'Copy Auth Link'}
                    </button>
                  </div>

                  <label style={{ marginTop: 20, fontSize: '0.75rem', fontWeight: 'bold', color: '#888', textTransform: 'uppercase' }}>
                    Browser did not redirect automatically? Paste the callback link or auth code here:
                  </label>
                  <div className="oauth-paste-box">
                    <input
                      type="text"
                      placeholder="Paste link or code here..."
                      value={manualLink}
                      onChange={(e) => setManualLink(e.target.value)}
                    />
                    <button type="button" onClick={handleManualSubmit} disabled={!manualLink}>
                      <Send size={16} /> Submit
                    </button>
                  </div>
                </div>
              )}
              {status.state === 'error' ? (
                <div style={{ color: '#ff6b6b', fontSize: '0.85rem', marginTop: '12px', textAlign: 'center' }}>
                  Sign-in failed. <button type="button" style={{ background: 'none', border: 'none', color: '#9fa6ff', cursor: 'pointer', textDecoration: 'underline', padding: 0, fontSize: 'inherit' }} onClick={onRefresh}>Try again</button>
                </div>
              ) : null}
            </div>
          ) : checking ? (
            <div className="discord-access-checking">
              <RefreshCw size={18} className="is-spinning" /> Contacting Discord securely...
            </div>
          ) : null}
        </div>
      </motion.section>
    </div>
  )
}
