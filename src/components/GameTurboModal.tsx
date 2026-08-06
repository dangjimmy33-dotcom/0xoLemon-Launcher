import { useState } from 'react'
import { Gauge, ShieldCheck, Zap } from 'lucide-react'
import './GameTurboModal.css'

interface GameTurboModalProps {
  gameName: string
  turboEnabled: boolean
  onEnable: () => void
  onDisable: () => void
  onClose: () => void
  onDontAskAgain: (enabled: boolean) => void
}

export function GameTurboModal({
  gameName,
  turboEnabled,
  onEnable,
  onDisable,
  onClose,
  onDontAskAgain,
}: GameTurboModalProps) {
  const [dontAsk, setDontAsk] = useState(false)

  function handleAction(enable: boolean) {
    if (enable) onEnable()
    else onDisable()
    if (dontAsk) onDontAskAgain(enable)
    onClose()
  }

  return (
    <div className="turbo-modal-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget) onClose()
    }}>
      <section className="turbo-modal" role="dialog" aria-modal="true" aria-labelledby="turbo-modal-title">
        <header className="turbo-modal-header">
          <span className="turbo-modal-icon" aria-hidden="true"><Zap size={19} /></span>
          <div>
            <span>GAME PERFORMANCE</span>
            <h2 id="turbo-modal-title">Game Turbo</h2>
          </div>
        </header>

        <p className="turbo-modal-desc">
          <strong>{gameName}</strong> đã khởi chạy. Game Turbo ưu tiên tài nguyên cho game trong phiên chơi hiện tại.
        </p>

        <div className="turbo-summary" aria-label="Game Turbo changes">
          <div>
            <Gauge size={17} />
            <span><strong>Process priority</strong><small>Chuyển tiến trình game sang mức High</small></span>
          </div>
          <div>
            <ShieldCheck size={17} />
            <span><strong>Launcher activity</strong><small>Giảm chuyển động giao diện khi game đang chạy</small></span>
          </div>
        </div>

        <label className="turbo-dont-ask">
          <input
            type="checkbox"
            checked={dontAsk}
            onChange={(event) => setDontAsk(event.target.checked)}
          />
          <span>Ghi nhớ lựa chọn này cho những lần sau</span>
        </label>

        <footer className="turbo-modal-actions">
          <button className="turbo-btn turbo-btn--skip" type="button" onClick={() => handleAction(false)}>
            Để sau
          </button>
          <button className="turbo-btn turbo-btn--enable" type="button" onClick={() => handleAction(true)}>
            <Zap size={15} />
            Bật Game Turbo
          </button>
        </footer>

        <small className="turbo-current-setting">
          Thiết lập hiện tại: {turboEnabled ? 'Tự động bật' : 'Hỏi trước khi bật'}
        </small>
      </section>
    </div>
  )
}
