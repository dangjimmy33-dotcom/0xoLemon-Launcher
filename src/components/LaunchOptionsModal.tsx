import { useMemo, useState } from 'react'
import { CheckCircle2, CircleAlert, Play, X } from 'lucide-react'
import type { ResolvedGameLaunchConfig } from '../types'

export function LaunchOptionsModal({
  gameTitle,
  config,
  onLaunch,
  onClose,
}: {
  gameTitle: string
  config: ResolvedGameLaunchConfig
  onLaunch: (optionId: string, optionTitle: string) => void
  onClose: () => void
}) {
  const initialOptionId = useMemo(() => {
    const configured = config.options.find(
      (option) => option.id === config.defaultOptionId && option.available,
    )
    return (
      configured ??
      config.options.find((option) => option.recommended && option.available) ??
      config.options.find((option) => option.available) ??
      config.options[0]
    )?.id ?? ''
  }, [config])
  const [selectedId, setSelectedId] = useState(initialOptionId)
  const effectiveSelectedId = config.options.some((option) => option.id === selectedId)
    ? selectedId
    : initialOptionId
  const selected = config.options.find((option) => option.id === effectiveSelectedId)
  const canLaunch = Boolean(selected?.available)

  return (
    <div
      className="dialog-backdrop launch-options-backdrop"
      role="presentation"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <section
        className="launch-options-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="launch-options-title"
      >
        <header className="launch-options-header">
          <div>
            <small>CHOOSE HOW TO LAUNCH</small>
            <h2 id="launch-options-title">{gameTitle}</h2>
            <p>The launcher will run the processes configured for this game.</p>
          </div>
          <button type="button" className="launch-options-close" onClick={onClose} aria-label="Close">
            <X size={18} />
          </button>
        </header>

        <div className="launch-options-list" role="radiogroup" aria-label="Launch options">
          {config.options.map((option) => {
            const selectedOption = option.id === effectiveSelectedId
            return (
              <button
                key={option.id}
                type="button"
                className={`launch-option-card${selectedOption ? ' selected' : ''}${option.available ? '' : ' unavailable'}`}
                role="radio"
                aria-checked={selectedOption}
                disabled={!option.available}
                onClick={() => setSelectedId(option.id)}
              >
                <span className="launch-option-radio" aria-hidden="true">
                  {selectedOption ? <CheckCircle2 size={21} /> : <span />}
                </span>
                <span className="launch-option-copy">
                  <strong>{option.title}</strong>
                  {option.description ? <small>{option.description}</small> : null}
                  {!option.available && option.unavailableReason ? (
                    <em>
                      <CircleAlert size={14} />
                      {option.unavailableReason}
                    </em>
                  ) : null}
                </span>
                {option.recommended ? <span className="launch-option-badge">Recommended</span> : null}
              </button>
            )
          })}
        </div>

        <footer className="launch-options-footer">
          <span>{config.source}</span>
          <div>
            <button type="button" className="secondary" onClick={onClose}>
              Cancel
            </button>
            <button
              type="button"
              className="primary-control launch-option-play"
              disabled={!canLaunch}
              onClick={() => {
                if (selected?.available) onLaunch(selected.id, selected.title)
              }}
            >
              <Play size={17} />
              Play
            </button>
          </div>
        </footer>
      </section>
    </div>
  )
}
