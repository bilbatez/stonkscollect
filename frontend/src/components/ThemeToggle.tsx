export type Theme = 'light' | 'dark'

/** Toggle between light/dark themes. */
export function ThemeToggle({ theme, onToggle }: { theme: Theme; onToggle: () => void }) {
  return (
    <button type="button" className="theme-toggle" onClick={onToggle}>
      {theme === 'dark' ? '☀ Light' : '🌙 Dark'}
    </button>
  )
}
