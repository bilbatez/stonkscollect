import { useEffect, useState, type FormEvent } from 'react'
import { Alert, Box, Button, Paper, Stack, TextField, Typography } from '@mui/material'
import { changePassword, getMe, updateProfile } from '../../api'

/** Account page: edit email + display name, and change password. Two
 *  independent forms, each with its own success / error feedback. */
export function Profile({ onProfileSaved }: { onProfileSaved?: () => void }) {
  const [email, setEmail] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [profileMsg, setProfileMsg] = useState<{ kind: 'ok' | 'error'; text: string } | null>(null)

  const [oldPassword, setOldPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [pwMsg, setPwMsg] = useState<{ kind: 'ok' | 'error'; text: string } | null>(null)

  useEffect(() => {
    void getMe().then((me) => {
      setEmail(me.email)
      setDisplayName(me.display_name)
    })
  }, [])

  async function saveProfile(e: FormEvent) {
    e.preventDefault()
    setProfileMsg(null)
    try {
      await updateProfile(email, displayName)
      setProfileMsg({ kind: 'ok', text: 'Profile saved.' })
      onProfileSaved?.()
    } catch (err) {
      setProfileMsg({ kind: 'error', text: err instanceof Error ? err.message : 'Save failed' })
    }
  }

  async function savePassword(e: FormEvent) {
    e.preventDefault()
    setPwMsg(null)
    if (newPassword !== confirm) {
      setPwMsg({ kind: 'error', text: 'New passwords do not match.' })
      return
    }
    try {
      await changePassword(oldPassword, newPassword)
      setPwMsg({ kind: 'ok', text: 'Password changed.' })
      setOldPassword('')
      setNewPassword('')
      setConfirm('')
    } catch (err) {
      setPwMsg({ kind: 'error', text: err instanceof Error ? err.message : 'Change failed' })
    }
  }

  return (
    <Stack spacing={3} sx={{ maxWidth: 480 }}>
      <Paper variant="outlined" sx={{ p: 3 }}>
        <Typography variant="h6" component="h2" gutterBottom>
          Profile
        </Typography>
        <Box component="form" onSubmit={saveProfile}>
          <Stack spacing={2}>
            <TextField
              type="email"
              label="Email"
              size="small"
              fullWidth
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              slotProps={{ htmlInput: { 'aria-label': 'profile email' } }}
            />
            <TextField
              label="Display name"
              size="small"
              fullWidth
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              slotProps={{ htmlInput: { 'aria-label': 'display name' } }}
            />
            <Button type="submit" variant="contained">
              Save profile
            </Button>
            {profileMsg && <Alert severity={profileMsg.kind === 'ok' ? 'success' : 'error'}>{profileMsg.text}</Alert>}
          </Stack>
        </Box>
      </Paper>

      <Paper variant="outlined" sx={{ p: 3 }}>
        <Typography variant="h6" component="h2" gutterBottom>
          Change password
        </Typography>
        <Box component="form" onSubmit={savePassword}>
          <Stack spacing={2}>
            <TextField
              type="password"
              label="Current password"
              size="small"
              fullWidth
              required
              value={oldPassword}
              onChange={(e) => setOldPassword(e.target.value)}
              slotProps={{ htmlInput: { 'aria-label': 'current password' } }}
            />
            <TextField
              type="password"
              label="New password"
              size="small"
              fullWidth
              required
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
              slotProps={{ htmlInput: { 'aria-label': 'new password' } }}
            />
            <TextField
              type="password"
              label="Confirm new password"
              size="small"
              fullWidth
              required
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
              slotProps={{ htmlInput: { 'aria-label': 'confirm password' } }}
            />
            <Button type="submit" variant="contained">
              Change password
            </Button>
            {pwMsg && <Alert severity={pwMsg.kind === 'ok' ? 'success' : 'error'}>{pwMsg.text}</Alert>}
          </Stack>
        </Box>
      </Paper>
    </Stack>
  )
}
