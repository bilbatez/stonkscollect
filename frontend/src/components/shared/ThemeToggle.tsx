import { Button } from '@mui/material'
import DarkModeIcon from '@mui/icons-material/DarkMode'
import LightModeIcon from '@mui/icons-material/LightMode'

export type Theme = 'light' | 'dark'

/** Toggle between light/dark themes. */
export function ThemeToggle({ theme, onToggle }: { theme: Theme; onToggle: () => void }) {
  const dark = theme === 'dark'
  return (
    <Button
      color="inherit"
      onClick={onToggle}
      startIcon={dark ? <LightModeIcon /> : <DarkModeIcon />}
    >
      {dark ? 'Light' : 'Dark'}
    </Button>
  )
}
