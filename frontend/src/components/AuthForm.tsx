import { useState, type FormEvent } from 'react'
import { Alert, Box, Button, Container, Link, Paper, Stack, TextField, Typography } from '@mui/material'
import { login, signup } from '../api'

/** Login / signup form. Calls `onAuth` with the token on success. */
export function AuthForm({ onAuth }: { onAuth: (token: string) => void }) {
  const [mode, setMode] = useState<'login' | 'signup'>('login')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')

  async function submit(e: FormEvent) {
    e.preventDefault()
    setError('')
    try {
      const token = mode === 'login' ? await login(email, password) : await signup(email, password)
      onAuth(token)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Request failed')
    }
  }

  return (
    <Container maxWidth="xs" sx={{ minHeight: '100vh', display: 'flex', alignItems: 'center' }}>
      <Paper elevation={3} sx={{ p: 4, width: '100%' }}>
        <Stack spacing={3}>
          <Typography variant="h4" component="h1" align="center" sx={{ fontWeight: 700 }}>
            StonksCollect
          </Typography>
          <Box component="form" onSubmit={submit}>
            <Stack spacing={2}>
              <TextField
                type="email"
                label="Email"
                placeholder="Email"
                size="small"
                fullWidth
                required
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                slotProps={{ htmlInput: { 'aria-label': 'email' } }}
              />
              <TextField
                type="password"
                label="Password"
                placeholder="Password"
                size="small"
                fullWidth
                required
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                slotProps={{ htmlInput: { 'aria-label': 'password' } }}
              />
              <Button type="submit" variant="contained" fullWidth>
                {mode === 'login' ? 'Log in' : 'Sign up'}
              </Button>
            </Stack>
          </Box>
          {error !== '' && <Alert severity="error">{error}</Alert>}
          <Link
            component="button"
            type="button"
            underline="hover"
            sx={{ textAlign: 'center' }}
            onClick={() => setMode(mode === 'login' ? 'signup' : 'login')}
          >
            {mode === 'login' ? 'Need an account? Sign up' : 'Have an account? Log in'}
          </Link>
        </Stack>
      </Paper>
    </Container>
  )
}
