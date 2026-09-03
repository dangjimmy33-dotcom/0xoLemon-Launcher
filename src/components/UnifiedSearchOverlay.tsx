import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { Clock3, Search, Sparkles, X } from 'lucide-react'

export type UnifiedSearchFilter = {
  id: string
  label: string
  count?: number | null
}

type UnifiedSearchOverlayProps = {
  open: boolean
  query: string
  onQueryChange: (value: string) => void
  onClose: () => void
  placeholder?: string
  ariaLabel?: string
  filters?: UnifiedSearchFilter[]
  activeFilter?: string
  onFilterChange?: (id: string) => void
  resultCount?: number
  resultsLabel?: string
  resultsHint?: string
  discoveryTitle?: string
  discoveryHint?: string
  historyKey?: string
  onSubmit?: (query: string) => void
  children: ReactNode
}

function normalizeTerm(value: string) {
  return value.trim().replace(/\s+/g, ' ')
}

function readHistory(key: string) {
  if (typeof window === 'undefined') return [] as string[]
  try {
    const raw = JSON.parse(window.localStorage.getItem(key) || '[]')
    return Array.isArray(raw)
      ? raw.map((value) => normalizeTerm(String(value))).filter(Boolean).slice(0, 8)
      : []
  } catch {
    return []
  }
}

export function UnifiedSearchOverlay({
  open,
  query,
  onQueryChange,
  onClose,
  placeholder = 'Search games, AppID, developer, tag, version...',
  ariaLabel = 'Search games',
  filters = [],
  activeFilter,
  onFilterChange,
  resultCount = 0,
  resultsLabel,
  resultsHint = 'Smart ranked results',
  discoveryTitle = 'Recommended for discovery',
  discoveryHint = 'Search by title or AppID to narrow the current catalog',
  historyKey = '0xo.unifiedGameSearchHistory',
  onSubmit,
  children,
}: UnifiedSearchOverlayProps) {
  const inputRef = useRef<HTMLInputElement>(null)
  const [history, setHistory] = useState<string[]>(() => readHistory(historyKey))
  const normalizedQuery = useMemo(() => normalizeTerm(query), [query])

  useEffect(() => {
    if (!open) return
    document.body.classList.add('store-search-overlay-open')
    const focusTimer = window.setTimeout(() => {
      inputRef.current?.focus()
      inputRef.current?.select()
    }, 30)
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.clearTimeout(focusTimer)
      window.removeEventListener('keydown', handleKeyDown)
      document.body.classList.remove('store-search-overlay-open')
    }
  }, [onClose, open])

  useEffect(() => {
    if (!open) setHistory(readHistory(historyKey))
  }, [historyKey, open])

  if (!open || typeof document === 'undefined') return null

  const remember = (value: string) => {
    const term = normalizeTerm(value)
    if (!term) return
    setHistory((current) => {
      const next = [term, ...current.filter((item) => item.toLowerCase() !== term.toLowerCase())].slice(0, 8)
      try { window.localStorage.setItem(historyKey, JSON.stringify(next)) } catch { /* best effort */ }
      return next
    })
  }

  const submit = () => {
    remember(normalizedQuery)
    onSubmit?.(normalizedQuery)
  }

  const chooseHistory = (term: string) => {
    onQueryChange(term)
    remember(term)
    onSubmit?.(term)
  }

  const clearHistory = () => {
    setHistory([])
    try { window.localStorage.removeItem(historyKey) } catch { /* best effort */ }
  }

  return createPortal(
    <div className="store-search-overlay unified-search-overlay" role="dialog" aria-modal="true" aria-label={ariaLabel}>
      <div className="store-search-backdrop" onClick={onClose} />
      <section className="store-search-surface">
        <header className="store-search-hero">
          <form
            className="store-search-command"
            onSubmit={(event) => {
              event.preventDefault()
              submit()
            }}
          >
            <Search size={22} />
            <input
              ref={inputRef}
              aria-label={ariaLabel}
              value={query}
              onChange={(event) => onQueryChange(event.target.value)}
              placeholder={placeholder}
              autoComplete="off"
              spellCheck="false"
            />
            {query ? (
              <button type="button" className="store-search-clear" onClick={() => onQueryChange('')} title="Clear search">
                <X size={18} />
              </button>
            ) : <kbd>Ctrl K</kbd>}
          </form>
          <button type="button" className="store-search-close" onClick={onClose} title="Close search">
            <X size={20} />
          </button>
        </header>

        {filters.length > 0 ? (
          <div className="store-search-filters" role="tablist" aria-label="Search filters">
            {filters.map((filter) => (
              <button
                key={filter.id}
                type="button"
                role="tab"
                aria-selected={activeFilter === filter.id}
                className={activeFilter === filter.id ? 'active' : ''}
                onClick={() => onFilterChange?.(filter.id)}
              >
                {filter.label}{typeof filter.count === 'number' ? ` (${filter.count})` : ''}
              </button>
            ))}
          </div>
        ) : null}

        {!normalizedQuery ? (
          <div className="store-search-discovery">
            <div className="store-search-discovery-columns">
              <section>
                <div className="store-search-section-title">
                  <span><Clock3 size={15} /> Recent searches</span>
                  {history.length ? <button type="button" onClick={clearHistory}>Clear</button> : null}
                </div>
                <div className="store-search-chips">
                  {history.length
                    ? history.map((term) => <button key={term} type="button" onClick={() => chooseHistory(term)}>{term}</button>)
                    : <p>Your recent searches will appear here.</p>}
                </div>
              </section>
              <section>
                <div className="store-search-section-title"><span><Sparkles size={15} /> Search tips</span></div>
                <div className="store-search-chips"><p>{discoveryHint}</p></div>
              </section>
            </div>
            <div className="store-search-results-heading">
              <span><Sparkles size={16} /> {discoveryTitle}</span>
              <small>{resultsHint}</small>
            </div>
          </div>
        ) : (
          <div className="store-search-results-heading">
            <span>{resultsLabel || `${resultCount} result${resultCount === 1 ? '' : 's'} for “${normalizedQuery}”`}</span>
            <small>{resultsHint}</small>
          </div>
        )}

        <div className="store-search-results" aria-live="polite">
          {children}
        </div>
      </section>
    </div>,
    document.body,
  )
}

type UnifiedSearchResultProps = {
  title: string
  subtitle?: string
  matchLabel?: string
  imageUrl?: string | null
  imageAlt?: string
  meta?: ReactNode
  onClick: () => void
}

export function UnifiedSearchResult({
  title,
  subtitle,
  matchLabel,
  imageUrl,
  imageAlt = '',
  meta,
  onClick,
}: UnifiedSearchResultProps) {
  return (
    <button type="button" className="store-search-result" onClick={onClick}>
      <div className="store-search-result-media">
        {imageUrl ? <img src={imageUrl} alt={imageAlt} loading="lazy" decoding="async" /> : <Search size={24} />}
      </div>
      <div className="store-search-result-copy">
        <strong>{title}</strong>
        {subtitle ? <span>{subtitle}</span> : null}
        {matchLabel ? <small>{matchLabel}</small> : null}
      </div>
      {meta ? <div className="store-search-result-meta">{meta}</div> : null}
    </button>
  )
}
