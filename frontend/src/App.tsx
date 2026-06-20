import { useEffect, useMemo, useState } from 'react'
import { CssBaseline, ThemeProvider, createTheme } from '@mui/material'
import { getSettings, getToken, logout } from './api'
import { AuthForm } from './components/auth/AuthForm'
import { Dashboard } from './components/layout/Dashboard'
import type { ThemePref } from './types'

const THEME_KEY = 'stonks_theme'

/** The persisted theme preference, defaulting to `system`. */
function storedPref(): ThemePref {
  const v = localStorage.getItem(THEME_KEY)
  return v === 'light' || v === 'dark' ? v : 'system'
}

/** Whether the OS currently prefers a dark color scheme. */
function systemPrefersDark(): boolean {
  return window.matchMedia('(prefers-color-scheme: dark)').matches
}

function App() {
  const [token, setTokenState] = useState<string | null>(getToken())
  const [pref, setPref] = useState<ThemePref>(storedPref)
  const [systemDark, setSystemDark] = useState<boolean>(systemPrefersDark)

  // Persist the preference for an instant boot next time.
  useEffect(() => {
    localStorage.setItem(THEME_KEY, pref)
  }, [pref])

  // Track OS scheme changes (only matters while pref is `system`).
  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => setSystemDark(mq.matches)
    mq.addEventListener('change', onChange)
    return () => mq.removeEventListener('change', onChange)
  }, [])

  // On login, adopt the user's saved theme preference.
  useEffect(() => {
    if (token === null) return
    void getSettings()
      .then((s) => setPref(s.theme))
      .catch(() => {})
  }, [token])

  const mode = pref === 'system' ? (systemDark ? 'dark' : 'light') : pref

  useEffect(() => {
    document.documentElement.dataset.theme = mode
  }, [mode])

  const muiTheme = useMemo(
    () =>
      createTheme({
        palette: {
          mode,
          primary: { main: '#14b8a6' },
          success: { main: '#22c55e' },
          error: { main: '#ef4444' },
          ...(mode === 'dark' ? { background: { default: '#0e1116', paper: '#161b22' } } : {}),
        },
      }),
    [mode],
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
          themePref={pref}
          onThemePref={setPref}
        />
      )}
    </ThemeProvider>
  )
}

export default App
