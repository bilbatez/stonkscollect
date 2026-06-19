import { useState, type FormEvent } from 'react'
import { Box, Button, Chip, IconButton, Paper, Stack, TextField, Typography } from '@mui/material'
import CloseIcon from '@mui/icons-material/Close'
import { formatNum, formatPct } from '../../format'
import type { WatchGroup, WatchQuote } from '../../types'
import { DataGrid } from '../shared/DataGrid'
import type { GridColumn } from '../shared/dataGridUtils'

interface Props {
  items: WatchQuote[]
  groups: WatchGroup[]
  onSelect: (ticker: string) => void
  onAdd: (ticker: string) => void
  onRemove: (ticker: string) => void
  onCreateGroup: (name: string) => void
  onRenameGroup: (id: number, name: string) => void
  onDeleteGroup: (id: number) => void
  onTag: (ticker: string, groupId: number) => void
  onUntag: (ticker: string, groupId: number) => void
}

/** Full-page watchlist: a sortable grid of watched stocks with price + day
 *  change, plus named groups (tags) to organize them. */
export function Watchlist({
  items,
  groups,
  onSelect,
  onAdd,
  onRemove,
  onCreateGroup,
  onRenameGroup,
  onDeleteGroup,
  onTag,
  onUntag,
}: Props) {
  const [ticker, setTicker] = useState('')
  const [newGroup, setNewGroup] = useState('')
  const [selected, setSelected] = useState<number[]>([])
  const [editing, setEditing] = useState<{ id: number; name: string } | null>(null)

  const groupName = new Map(groups.map((g) => [g.id, g.name]))

  function add(e: FormEvent) {
    e.preventDefault()
    const t = ticker.trim().toUpperCase()
    if (t !== '') {
      onAdd(t)
      setTicker('')
    }
  }

  function create(e: FormEvent) {
    e.preventDefault()
    const name = newGroup.trim()
    if (name !== '') {
      onCreateGroup(name)
      setNewGroup('')
    }
  }

  function toggleFilter(id: number) {
    setSelected((s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id]))
  }

  function saveRename(e: FormEvent) {
    e.preventDefault()
    if (editing && editing.name.trim() !== '') {
      onRenameGroup(editing.id, editing.name.trim())
      setEditing(null)
    }
  }

  // Rows in any of the selected groups (or all rows when nothing is selected).
  const shown =
    selected.length === 0
      ? items
      : items.filter((q) => q.group_ids.some((g) => selected.includes(g)))

  const columns: GridColumn<WatchQuote>[] = [
    {
      id: 'ticker',
      header: 'Ticker',
      sortValue: (q) => q.company.ticker,
      filter: true,
      cell: (q) => (
        <Button size="small" onClick={() => onSelect(q.company.ticker)}>
          {q.company.ticker}
        </Button>
      ),
    },
    { id: 'name', header: 'Name', sortValue: (q) => q.company.name, cell: (q) => q.company.name },
    {
      id: 'last',
      header: 'Last',
      sortValue: (q) => q.last_close ?? -1,
      cell: (q) => (q.last_close !== null ? formatNum(q.last_close) : '—'),
    },
    {
      id: 'change',
      header: 'Change',
      sortValue: (q) => q.change_pct ?? 0,
      cell: (q) =>
        q.change !== null && q.change_pct !== null ? (
          <Chip
            size="small"
            variant="outlined"
            color={q.change >= 0 ? 'success' : 'error'}
            label={`${q.change >= 0 ? '+' : ''}${formatPct(q.change_pct)}`}
          />
        ) : (
          '—'
        ),
    },
    {
      id: 'volume',
      header: 'Volume',
      sortValue: (q) => q.volume ?? -1,
      cell: (q) => (q.volume !== null ? formatNum(q.volume) : '—'),
    },
    {
      id: 'groups',
      header: 'Groups',
      cell: (q) => (
        <Stack direction="row" spacing={0.5} sx={{ flexWrap: 'wrap', alignItems: 'center' }}>
          {q.group_ids.map((gid) => (
            <Chip
              key={gid}
              size="small"
              label={groupName.get(gid) ?? gid}
              onDelete={() => onUntag(q.company.ticker, gid)}
            />
          ))}
          {groups
            .filter((g) => !q.group_ids.includes(g.id))
            .map((g) => (
              <Button
                key={g.id}
                size="small"
                variant="text"
                aria-label={`tag ${q.company.ticker} into ${g.name}`}
                onClick={() => onTag(q.company.ticker, g.id)}
              >
                +{g.name}
              </Button>
            ))}
        </Stack>
      ),
    },
    {
      id: 'remove',
      header: '',
      cell: (q) => (
        <IconButton
          size="small"
          aria-label={`remove ${q.company.ticker}`}
          onClick={() => onRemove(q.company.ticker)}
        >
          <CloseIcon fontSize="small" />
        </IconButton>
      ),
    },
  ]

  return (
    <Box>
      <Typography variant="h6" component="h2" gutterBottom>
        Watchlist
      </Typography>

      <Stack direction="row" spacing={2} sx={{ mb: 2, flexWrap: 'wrap', alignItems: 'center' }}>
        <Box component="form" onSubmit={add}>
          <Stack direction="row" spacing={1}>
            <TextField
              size="small"
              placeholder="Add ticker"
              value={ticker}
              onChange={(e) => setTicker(e.target.value)}
              slotProps={{ htmlInput: { 'aria-label': 'add ticker' } }}
            />
            <Button type="submit" variant="contained">
              Add
            </Button>
          </Stack>
        </Box>
        <Box component="form" onSubmit={create}>
          <Stack direction="row" spacing={1}>
            <TextField
              size="small"
              placeholder="New group"
              value={newGroup}
              onChange={(e) => setNewGroup(e.target.value)}
              slotProps={{ htmlInput: { 'aria-label': 'new group' } }}
            />
            <Button type="submit" variant="outlined">
              Create
            </Button>
          </Stack>
        </Box>
      </Stack>

      {groups.length > 0 && (
        <Stack
          direction="row"
          spacing={1}
          role="group"
          aria-label="group filters"
          sx={{ mb: 2, flexWrap: 'wrap', alignItems: 'center' }}
        >
          <Chip
            label="All"
            color={selected.length === 0 ? 'primary' : 'default'}
            onClick={() => setSelected([])}
          />
          {groups.map((g) =>
            editing && editing.id === g.id ? (
              <Box component="form" onSubmit={saveRename} key={g.id}>
                <TextField
                  size="small"
                  value={editing.name}
                  onChange={(e) => setEditing({ id: g.id, name: e.target.value })}
                  slotProps={{ htmlInput: { 'aria-label': `rename ${g.name}` } }}
                />
              </Box>
            ) : (
              <Stack key={g.id} direction="row" spacing={0.5} sx={{ alignItems: 'center' }}>
                <Chip
                  label={g.name}
                  color={selected.includes(g.id) ? 'primary' : 'default'}
                  onClick={() => toggleFilter(g.id)}
                  onDelete={() => onDeleteGroup(g.id)}
                />
                <Button size="small" aria-label={`edit ${g.name}`} onClick={() => setEditing({ id: g.id, name: g.name })}>
                  Rename
                </Button>
              </Stack>
            ),
          )}
        </Stack>
      )}

      {items.length === 0 ? (
        <Paper variant="outlined" sx={{ p: 2 }}>
          <Typography color="text.secondary">No tickers yet.</Typography>
        </Paper>
      ) : (
        <DataGrid columns={columns} rows={shown} getRowId={(q) => q.company.ticker} empty="No matches." />
      )}
    </Box>
  )
}
