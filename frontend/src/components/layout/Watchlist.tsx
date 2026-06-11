import { useState, type FormEvent } from 'react'
import {
  Box,
  Button,
  IconButton,
  List,
  ListItem,
  ListItemButton,
  ListItemText,
  Paper,
  Stack,
  TextField,
  Typography,
} from '@mui/material'
import CloseIcon from '@mui/icons-material/Close'
import type { Company } from '../../types'

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
    <Paper
      component="aside"
      variant="outlined"
      sx={{ p: 2, width: 240, flexShrink: 0, alignSelf: 'flex-start' }}
    >
      <Typography variant="h6" component="h2" gutterBottom>
        Watchlist
      </Typography>
      <Box component="form" onSubmit={add}>
        <Stack direction="row" spacing={1}>
          <TextField
            size="small"
            placeholder="Add ticker"
            value={ticker}
            onChange={(e) => setTicker(e.target.value)}
            slotProps={{ htmlInput: { 'aria-label': 'add ticker' } }}
            fullWidth
          />
          <Button type="submit" variant="contained">
            Add
          </Button>
        </Stack>
      </Box>
      {items.length === 0 ? (
        <Typography color="text.secondary" sx={{ mt: 2 }}>
          No tickers yet.
        </Typography>
      ) : (
        <List dense sx={{ mt: 1 }}>
          {items.map((c) => (
            <ListItem
              key={c.ticker}
              disablePadding
              secondaryAction={
                <IconButton
                  edge="end"
                  size="small"
                  aria-label={`remove ${c.ticker}`}
                  onClick={() => onRemove(c.ticker)}
                >
                  <CloseIcon fontSize="small" />
                </IconButton>
              }
            >
              <ListItemButton onClick={() => onSelect(c.ticker)}>
                <ListItemText primary={c.ticker} />
              </ListItemButton>
            </ListItem>
          ))}
        </List>
      )}
    </Paper>
  )
}
