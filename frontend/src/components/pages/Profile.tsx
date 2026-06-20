import { useEffect, useState, type FormEvent } from 'react'
import { Alert, Box, Button, MenuItem, Paper, Stack, TextField, Typography } from '@mui/material'
import { changePassword, getMe, getSettings, updateProfile, updateSettings } from '../../api'
import type { GrahamConfig, ThemePref } from '../../types'

type Msg = { kind: 'ok' | 'error'; text: string } | null

/** The editable Graham threshold fields, with display labels. */
const GRAHAM_FIELDS: { key: keyof GrahamConfig; label: string }[] = [
  { key: 'min_revenue', label: 'Min revenue ($)' },
  { key: 'pe_max', label: 'Max P/E' },
  { key: 'pb_max', label: 'Max P/B' },
  { key: 'pe_pb_max', label: 'Max P/E × P/B' },
  { key: 'current_ratio_min', label: 'Min current ratio' },
  { key: 'eps_growth_min', label: 'Min EPS growth (fraction)' },
]

/** Account page: edit profile, change password, and set preferences (theme +
 *  Graham thresholds). Each section has its own submit + feedback. */
export function Profile({
  themePref,
  onThemePref,
  onProfileSaved,
}: {
  themePref: ThemePref
  onThemePref: (pref: ThemePref) => void
  onProfileSaved?: () => void
}) {
  const [email, setEmail] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [profileMsg, setProfileMsg] = useState<Msg>(null)

  const [oldPassword, setOldPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [pwMsg, setPwMsg] = useState<Msg>(null)

  const [graham, setGraham] = useState<GrahamConfig>({
    min_revenue: 0,
    pe_max: 0,
    pb_max: 0,
    pe_pb_max: 0,
    current_ratio_min: 0,
    eps_growth_min: 0,
  })
  const [prefMsg, setPrefMsg] = useState<Msg>(null)

  useEffect(() => {
    void getMe().then((me) => {
      setEmail(me.email)
      setDisplayName(me.display_name)
    })
    void getSettings().then((s) => setGraham(s.graham))
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

  async function savePreferences(e: FormEvent) {
    e.preventDefault()
    setPrefMsg(null)
    try {
      await updateSettings({ theme: themePref, graham })
      setPrefMsg({ kind: 'ok', text: 'Preferences saved.' })
    } catch (err) {
      setPrefMsg({ kind: 'error', text: err instanceof Error ? err.message : 'Save failed' })
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
          Preferences
        </Typography>
        <Box component="form" onSubmit={savePreferences}>
          <Stack spacing={2}>
            <TextField
              select
              label="Theme"
              size="small"
              fullWidth
              value={themePref}
              onChange={(e) => onThemePref(e.target.value as ThemePref)}
            >
              <MenuItem value="system">System (match device)</MenuItem>
              <MenuItem value="light">Light</MenuItem>
              <MenuItem value="dark">Dark</MenuItem>
            </TextField>
            <Typography variant="subtitle2" color="text.secondary">
              Graham defensive thresholds
            </Typography>
            {GRAHAM_FIELDS.map(({ key, label }) => (
              <TextField
                key={key}
                type="number"
                label={label}
                size="small"
                fullWidth
                value={graham[key]}
                onChange={(e) => setGraham({ ...graham, [key]: Number(e.target.value) })}
                slotProps={{ htmlInput: { 'aria-label': key } }}
              />
            ))}
            <Button type="submit" variant="contained">
              Save preferences
            </Button>
            {prefMsg && <Alert severity={prefMsg.kind === 'ok' ? 'success' : 'error'}>{prefMsg.text}</Alert>}
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
