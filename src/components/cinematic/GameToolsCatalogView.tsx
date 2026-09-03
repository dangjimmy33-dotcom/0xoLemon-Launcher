import { useMemo, useState } from 'react'
import { ChevronLeft, ChevronRight, Gamepad2, LoaderCircle, Search } from 'lucide-react'
import type { GameToolsCatalogItem } from '../../types'
import './cinematic.css'

type GameToolsCatalogViewProps = {
  title: string
  description: string
  items: readonly GameToolsCatalogItem[]
  categories: readonly string[]
  loading: boolean
  onSelect: (item: GameToolsCatalogItem) => void
}

const PAGE_SIZE = 20

export default function GameToolsCatalogView({ title, description, items, categories, loading, onSelect }: GameToolsCatalogViewProps) {
  const [query, setQuery] = useState('')
  const [category, setCategory] = useState('all')
  const [page, setPage] = useState(0)
  const filtered = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    return items.filter((item) => {
      if (category !== 'all' && item.category !== category) return false
      return !normalized || item.name.toLocaleLowerCase().includes(normalized) || String(item.appId).includes(normalized)
    })
  }, [category, items, query])
  const pageCount = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE))
  const visiblePage = Math.min(page, pageCount - 1)
  const visible = filtered.slice(visiblePage * PAGE_SIZE, (visiblePage + 1) * PAGE_SIZE)

  return (
    <section className="cinematic-generic-catalog">
      <header className="cinematic-view-heading">
        <div><span>0XOLEMON GAME TOOLS</span><h1>{title}</h1><p>{description}</p></div>
      </header>
      <div className="cinematic-catalog-toolbar">
        <label><Search /><input value={query} onChange={(event) => { setQuery(event.target.value); setPage(0) }} placeholder="Search name or AppID" /></label>
        {categories.length > 0 ? <select value={category} onChange={(event) => { setCategory(event.target.value); setPage(0) }}><option value="all">All categories</option>{categories.map((value) => <option key={value} value={value}>{value}</option>)}</select> : null}
        <span>{filtered.length} results</span>
      </div>
      {loading && items.length === 0 ? <div className="cinematic-provider-loading"><LoaderCircle className="spin" /> Loading verified catalog…</div> : null}
      <div className="cinematic-generic-grid">
        {visible.map((item) => (
          <button key={`${item.kind}-${item.appId}`} type="button" onClick={() => onSelect(item)}>
            <span>{item.imageUrl ? <img src={item.imageUrl} alt="" loading="lazy" decoding="async" /> : <Gamepad2 />}</span>
            <strong>{item.name}</strong><small>AppID {item.appId}</small><i>{item.category ?? item.kind}</i>
          </button>
        ))}
      </div>
      {visible.length === 0 && !loading ? <div className="cinematic-empty"><Search /><strong>No matching entries</strong><span>Try another game, AppID or category.</span></div> : null}
      <footer className="cinematic-pagination"><button type="button" disabled={visiblePage === 0} onClick={() => setPage((value) => Math.max(0, value - 1))}><ChevronLeft /> Previous</button><span>Page {visiblePage + 1} / {pageCount}</span><button type="button" disabled={visiblePage + 1 >= pageCount} onClick={() => setPage((value) => Math.min(pageCount - 1, value + 1))}>Next <ChevronRight /></button></footer>
    </section>
  )
}
