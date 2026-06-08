import { useState, type FormEvent } from 'react'
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
    } catch {
      setError(mode === 'login' ? 'Login failed' : 'Signup failed')
    }
  }

  return (
    <main className="auth">
      <h1>StonksCollect</h1>
      <form onSubmit={submit}>
        <input
          aria-label="email"
          type="email"
          placeholder="Email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
        <input
          aria-label="password"
          type="password"
          placeholder="Password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <button type="submit">{mode === 'login' ? 'Log in' : 'Sign up'}</button>
      </form>
      {error !== '' && <p role="alert">{error}</p>}
      <button
        type="button"
        className="link"
        onClick={() => setMode(mode === 'login' ? 'signup' : 'login')}
      >
        {mode === 'login' ? 'Need an account? Sign up' : 'Have an account? Log in'}
      </button>
    </main>
  )
}
