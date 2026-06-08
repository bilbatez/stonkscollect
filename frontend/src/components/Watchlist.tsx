import { useState, type FormEvent } from 'react'
import type { Company } from '../types'

interface Props {
  items: Company[]
  onSelect: (ticker: string) => void
  onAdd: (ticker: string) => void
  onRemove: (ticker: string) => void
}

/** Sidebar watchlist with add input and per-item select/remove. */
export function Watchlist({ items, onSelect, onAdd, onRemove }: Props) {
  const [ticker, setTicker] = useState('')

  function add(e: FormEvent) {
    e.preventDefault()
    const t = ticker.trim().toUpperCase()
    if (t !== '') {
      onAdd(t)
      setTicker('')
    }
  }

  return (
    <aside className="watchlist">
      <h2>Watchlist</h2>
      <form onSubmit={add}>
        <input
          aria-label="add ticker"
          placeholder="Add ticker"
          value={ticker}
          onChange={(e) => setTicker(e.target.value)}
        />
        <button type="submit">Add</button>
      </form>
      {items.length === 0 ? (
        <p>No tickers yet.</p>
      ) : (
        <ul>
          {items.map((c) => (
            <li key={c.ticker}>
              <button type="button" className="link" onClick={() => onSelect(c.ticker)}>
                {c.ticker}
              </button>
              <button
                type="button"
                aria-label={`remove ${c.ticker}`}
                onClick={() => onRemove(c.ticker)}
              >
                ✕
              </button>
            </li>
          ))}
        </ul>
      )}
    </aside>
  )
}
