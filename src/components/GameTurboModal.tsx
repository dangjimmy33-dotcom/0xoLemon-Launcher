/**
 * GameTurboModal — Hiển thị dialog first-run giải thích Game Turbo.
 * Người dùng có thể:
 *  - Bật Turbo ngay
 *  - Tắt
 *  - "Không hỏi lại" (lưu preference)
 */

import { useState } from 'react'
import { Zap } from 'lucide-react'
import './GameTurboModal.css'

interface GameTurboModalProps {
  /** Tên game đang chạy */
  gameName: string
  /** Turbo có đang bật không */
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
    <div className="turbo-modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="turbo-modal-title">
      <div className="turbo-modal">
        <div className="turbo-modal-icon">
          <Zap size={28} />
        </div>

        <h2 id="turbo-modal-title" className="turbo-modal-title">
          Game Turbo
        </h2>

        <p className="turbo-modal-desc">
          <strong className="turbo-game-name">{gameName}</strong> đang chạy.
          <br />
          Bật <strong>Game Turbo</strong> để tăng độ ưu tiên xử lý cho game,
          giảm giật lag và tối ưu hiệu năng.
        </p>

        <ul className="turbo-feature-list">
          <li>
            <Zap size={13} />
            Tăng process priority của game lên <strong>High</strong>
          </li>
          <li>
            <Zap size={13} />
            Giảm animations launcher để tiết kiệm GPU
          </li>
          <li>
            <Zap size={13} />
            Tự động tắt khi game đóng
          </li>
        </ul>

        <div className="turbo-modal-actions">
          <button
            className="turbo-btn turbo-btn--enable"
            onClick={() => handleAction(true)}
          >
            <Zap size={15} />
            Bật Game Turbo
          </button>
          <button
            className="turbo-btn turbo-btn--skip"
            onClick={() => handleAction(false)}
          >
            Để lúc khác
          </button>
        </div>

        <label className="turbo-dont-ask">
          <input
            type="checkbox"
            checked={dontAsk}
            onChange={(e) => setDontAsk(e.target.checked)}
          />
          <span>Không hỏi lại (sẽ tự {turboEnabled ? 'bật' : 'tắt'} theo lựa chọn này)</span>
        </label>
      </div>
    </div>
  )
}
