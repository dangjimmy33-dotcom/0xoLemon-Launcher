import type { LaunchSplashState } from '../types'

export function LaunchSplash({ splash }: { splash: LaunchSplashState }) {
  return (
    <div className="launch-splash" role="status" aria-live="polite">
      <section className="launch-splash-card">
        {splash.heroUrl ? <img className="launch-splash-hero" src={splash.heroUrl} alt="" /> : null}
        <div className="launch-splash-shade" />
        <div className="launch-splash-content">
          {splash.iconUrl ? <img className="launch-splash-icon" src={splash.iconUrl} alt="" /> : null}
          <div>
            <strong>{splash.title}</strong>
            <span>Chúc bạn chơi game vui vẻ</span>
          </div>
        </div>
      </section>
    </div>
  )
}
