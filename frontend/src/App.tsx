import { useEffect, useMemo, useState } from 'react'
import { CssBaseline, ThemeProvider, createTheme } from '@mui/material'
import { getToken, logout } from './api'
import { AuthForm } from './components/AuthForm'
import { Dashboard } from './components/Dashboard'
import { type Theme } from './components/ThemeToggle'

const THEME_KEY = 'stonks_theme'

/** The persisted theme, defaulting to dark when none is stored. */
function storedTheme(): Theme {
  return localStorage.getItem(THEME_KEY) === 'light' ? 'light' : 'dark'
}

function App() {
  const [token, setTokenState] = useState<string | null>(getToken())
  const [theme, setTheme] = useState<Theme>(storedTheme)

  useEffect(() => {
    document.documentElement.dataset.theme = theme
    localStorage.setItem(THEME_KEY, theme)
  }, [theme])

  const muiTheme = useMemo(
    () =>
      createTheme({
        palette: {
          mode: theme,
          primary: { main: '#14b8a6' },
          success: { main: '#22c55e' },
          error: { main: '#ef4444' },
          ...(theme === 'dark'
            ? { background: { default: '#0e1116', paper: '#161b22' } }
            : {}),
        },
      }),
    [theme],
  )

  return (
    <ThemeProvider theme={muiTheme}>
      <CssBaseline />
      {token === null ? (
        <AuthForm onAuth={setTokenState} />
      ) : (
        <Dashboard
          onLogout={() => {
            void logout()
            setTokenState(null)
          }}
          theme={theme}
          onToggleTheme={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
        />
      )}
    </ThemeProvider>
  )
}

export default App
