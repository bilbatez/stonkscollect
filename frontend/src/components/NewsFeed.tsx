import type { NewsItem } from '../types'

/** Newest-first list of headlines (title + description only). */
export function NewsFeed({ news }: { news: NewsItem[] }) {
  if (news.length === 0) {
    return <p>No news.</p>
  }
  return (
    <ul className="news">
      {news.map((n) => (
        <li key={n.dedup_hash}>
          <a href={n.url} target="_blank" rel="noreferrer">
            {n.title}
          </a>
          <span className="news-source">{n.source}</span>
          {n.description !== null && <p className="news-desc">{n.description}</p>}
        </li>
      ))}
    </ul>
  )
}
