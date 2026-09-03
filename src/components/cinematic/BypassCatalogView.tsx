import { useMemo, useState, type CSSProperties } from 'react'
import { ArrowLeft, Gamepad2, Search } from 'lucide-react'
import type { GameToolsCatalogItem } from '../../types'
import { BYPASS_PROVIDERS, itemsForProvider, type BypassProviderId } from './bypassProviders'
import './cinematic.css'

type BypassCatalogViewProps = {
  items: readonly GameToolsCatalogItem[]
  providerId: BypassProviderId
  onProviderChange: (provider: BypassProviderId) => void
  onBack: () => void
  onSelect: (item: GameToolsCatalogItem) => void
}

export default function BypassCatalogView({ items, providerId, onProviderChange, onBack, onSelect }: BypassCatalogViewProps) {
  const [query, setQuery] = useState('')
  const provider = BYPASS_PROVIDERS.find((candidate) => candidate.id === providerId) ?? BYPASS_PROVIDERS[0]
  const providerItems = useMemo(() => itemsForProvider(items, providerId), [items, providerId])
  const filtered = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    if (!normalized) return providerItems
    return providerItems.filter((item) => item.name.toLocaleLowerCase().includes(normalized) || String(item.appId).includes(normalized))
  }, [providerItems, query])

  return (
    <section className="cinematic-bypass-catalog" aria-labelledby="cinematic-catalog-title">
      <nav className="cinematic-provider-rail" aria-label="Bypass providers">
        <button type="button" className="cinematic-provider-back" onClick={onBack}><ArrowLeft /> Providers</button>
        {BYPASS_PROVIDERS.map((candidate) => {
          const count = itemsForProvider(items, candidate.id).length
          return (
            <button
              key={candidate.id}
              type="button"
              className={candidate.id === providerId ? 'is-active' : ''}
              style={{ '--provider-accent': candidate.accent } as CSSProperties}
              disabled={count === 0}
              onClick={() => { setQuery(''); onProviderChange(candidate.id) }}
            >
              <b>{candidate.monogram}</b><span>{candidate.label}</span><small>{count}</small>
            </button>
          )
        })}
      </nav>
      <header className="cinematic-catalog-header">
        <div>
          <span>{provider.description}</span>
          <h1 id="cinematic-catalog-title">{provider.label}</h1>
          <p>{providerItems.length} verified compatibility packages</p>
        </div>
        <label><Search /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search name or AppID" /></label>
      </header>
      <div className="cinematic-bypass-grid">
        {filtered.map((item, index) => (
          <button
            key={`${item.appId}-${item.packageName ?? item.name}`}
            type="button"
            className="cinematic-bypass-card"
            style={{ '--card-order': index, contentVisibility: 'auto', containIntrinsicSize: '340px' } as CSSProperties}
            onClick={() => onSelect(item)}
          >
            <span className="cinematic-bypass-art">
              {item.imageUrl ? <img src={item.imageUrl} alt="" loading="lazy" decoding="async" /> : <Gamepad2 />}
            </span>
            <span className="cinematic-bypass-copy"><strong>{item.name}</strong><small>AppID {item.appId}</small></span>
            <span className="cinematic-bypass-open">Open details</span>
          </button>
        ))}
      </div>
      {filtered.length === 0 ? <div className="cinematic-empty"><Search /><strong>No matching games</strong><span>Try another title or AppID.</span></div> : null}
    </section>
  )
}
