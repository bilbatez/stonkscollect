import { useState } from 'react'
import { Button, Stack, TextField, Typography } from '@mui/material'
import * as api from '../../api'

interface Props {
  ticker: string
  initialBody: string | null
}

export function NotePanel({ ticker, initialBody }: Props) {
  const [body, setBody] = useState(initialBody ?? '')
  const [saved, setSaved] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function handleSave() {
    try {
      await api.saveNote(ticker, body)
      setSaved(true)
      setError(null)
    } catch {
      setError('Failed to save note.')
    }
  }

  async function handleDelete() {
    try {
      await api.deleteNote(ticker)
      setBody('')
      setSaved(false)
      setError(null)
    } catch {
      setError('Failed to delete note.')
    }
  }

  return (
    <Stack spacing={1}>
      <TextField
        multiline
        minRows={3}
        fullWidth
        value={body}
        onChange={(e) => {
          setBody(e.target.value)
          setSaved(false)
        }}
        placeholder="Your notes about this company…"
        size="small"
      />
      {error && (
        <Typography variant="caption" color="error">
          {error}
        </Typography>
      )}
      {saved && (
        <Typography variant="caption" color="success.main">
          Saved.
        </Typography>
      )}
      <Stack direction="row" spacing={1}>
        <Button size="small" variant="contained" onClick={handleSave} disabled={body === ''}>
          Save
        </Button>
        <Button size="small" variant="outlined" color="error" onClick={handleDelete} disabled={body === ''}>
          Delete
        </Button>
      </Stack>
    </Stack>
  )
}
