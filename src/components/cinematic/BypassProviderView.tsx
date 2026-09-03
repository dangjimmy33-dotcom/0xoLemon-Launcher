import { useEffect, useMemo, useState, type CSSProperties } from 'react'
import { ArrowRight, LoaderCircle } from 'lucide-react'
import type { GameToolsCatalogItem } from '../../types'
import {
  BYPASS_PROVIDERS,
  itemsForProvider,
  providerHero,
  type BypassProviderId,
} from './bypassProviders'
import './cinematic.css'

type BypassProviderViewProps = {
  items: readonly GameToolsCatalogItem[]
  loading: boolean
  onSelect: (provider: BypassProviderId) => void
}

export default function BypassProviderView({ items, loading, onSelect }: BypassProviderViewProps) {
  const firstAvailable = useMemo(() => BYPASS_PROVIDERS.find((provider) => itemsForProvider(items, provider.id).length > 0)?.id ?? 'ubisoft', [items])
  const [focused, setFocused] = useState<BypassProviderId>(firstAvailable)
  const activeHero = providerHero(items, focused)

  useEffect(() => setFocused(firstAvailable), [firstAvailable])
  useEffect(() => {
    const index = BYPASS_PROVIDERS.findIndex((provider) => provider.id === focused)
    const next = BYPASS_PROVIDERS[(index + 1) % BYPASS_PROVIDERS.length]
    const url = next ? providerHero(items, next.id) : null
    if (!url) return
    const image = new Image()
    image.decoding = 'async'
    image.src = url
  }, [focused, items])

  return (
    <section className="cinematic-provider-view" aria-labelledby="cinematic-provider-title">
      <div className="cinematic-provider-background" aria-hidden="true">
        {activeHero ? <img key={activeHero} src={activeHero} alt="" decoding="async" /> : null}
        <div />
      </div>
      <header>
        <span>0XOLEMON COMPATIBILITY</span>
        <h1 id="cinematic-provider-title">Choose a provider</h1>
        <p>Packages are validated and applied transactionally after you select the game folder.</p>
      </header>
      {loading && items.length === 0 ? <div className="cinematic-provider-loading"><LoaderCircle className="spin" /> Loading verified catalog…</div> : null}
      <div className="cinematic-provider-grid" aria-label="Bypass providers">
        {BYPASS_PROVIDERS.map((provider) => {
          const count = itemsForProvider(items, provider.id).length
          return (
            <button
              key={provider.id}
              type="button"
              className={focused === provider.id ? 'is-focused' : ''}
              style={{ '--provider-accent': provider.accent } as CSSProperties}
              disabled={count === 0}
              title={count === 0 ? `${provider.label} has no packages in the current verified catalog.` : undefined}
              onMouseEnter={() => setFocused(provider.id)}
              onFocus={() => setFocused(provider.id)}
              onClick={() => onSelect(provider.id)}
            >
              <span className="cinematic-provider-spotlight" />
              <b>{provider.monogram}</b>
              <strong>{provider.label}</strong>
              <small>{count > 0 ? `${count} games` : 'Unavailable'}</small>
              <i>{provider.description}</i>
              <ArrowRight />
            </button>
          )
        })}
      </div>
    </section>
  )
}
