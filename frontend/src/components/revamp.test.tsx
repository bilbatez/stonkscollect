import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, test, vi } from 'vitest'
import { AuthForm } from './AuthForm'
import { Compare } from './Compare'
import { Skeleton } from './Skeleton'
import { ThemeToggle } from './ThemeToggle'
import { Watchlist } from './Watchlist'
import * as api from '../api'
import type { Company } from '../types'

vi.mock('../api')

afterEach(() => vi.clearAllMocks())

const company = (ticker: string): Company => ({
  id: 1,
  cik: '',
  ticker,
  name: ticker,
  exchange: null,
  sector: null,
  industry: null,
})

test('AuthForm logs in and reports the token', async () => {
  vi.mocked(api.login).mockResolvedValue('tok')
  const onAuth = vi.fn()
  render(<AuthForm onAuth={onAuth} />)
  await userEvent.type(screen.getByLabelText('email'), 'a@e.com')
  await userEvent.type(screen.getByLabelText('password'), 'pw')
  await userEvent.click(screen.getByRole('button', { name: /log in/i }))
  await waitFor(() => expect(onAuth).toHaveBeenCalledWith('tok'))
})

test('AuthForm surfaces a login error', async () => {
  vi.mocked(api.login).mockRejectedValue(new Error('nope'))
  render(<AuthForm onAuth={vi.fn()} />)
  await userEvent.click(screen.getByRole('button', { name: /log in/i }))
  expect(await screen.findByRole('alert')).toHaveTextContent(/login failed/i)
})

test('AuthForm can switch to signup and back, and surfaces signup errors', async () => {
  vi.mocked(api.signup).mockRejectedValue(new Error('nope'))
  const onAuth = vi.fn()
  render(<AuthForm onAuth={onAuth} />)
  await userEvent.click(screen.getByRole('button', { name: /need an account/i }))
  await userEvent.click(screen.getByRole('button', { name: /sign up/i }))
  expect(await screen.findByRole('alert')).toHaveTextContent(/signup failed/i)
  expect(onAuth).not.toHaveBeenCalled()
  // toggle back to login
  await userEvent.click(screen.getByRole('button', { name: /have an account/i }))
  expect(screen.getByRole('button', { name: /log in/i })).toBeInTheDocument()
})

test('AuthForm signup success reports the token', async () => {
  vi.mocked(api.signup).mockResolvedValue('newtok')
  const onAuth = vi.fn()
  render(<AuthForm onAuth={onAuth} />)
  await userEvent.click(screen.getByRole('button', { name: /need an account/i }))
  await userEvent.type(screen.getByLabelText('email'), 'a@e.com')
  await userEvent.type(screen.getByLabelText('password'), 'pw')
  await userEvent.click(screen.getByRole('button', { name: /sign up/i }))
  await waitFor(() => expect(onAuth).toHaveBeenCalledWith('newtok'))
})

test('Watchlist selects, adds (trimmed/upper), ignores blanks, removes', async () => {
  const onSelect = vi.fn()
  const onAdd = vi.fn()
  const onRemove = vi.fn()
  render(
    <Watchlist items={[company('AAPL')]} onSelect={onSelect} onAdd={onAdd} onRemove={onRemove} />,
  )
  await userEvent.click(screen.getByRole('button', { name: 'AAPL' }))
  expect(onSelect).toHaveBeenCalledWith('AAPL')
  await userEvent.click(screen.getByRole('button', { name: 'remove AAPL' }))
  expect(onRemove).toHaveBeenCalledWith('AAPL')
  await userEvent.type(screen.getByLabelText('add ticker'), ' msft ')
  await userEvent.click(screen.getByRole('button', { name: 'Add' }))
  expect(onAdd).toHaveBeenCalledWith('MSFT')
  // blank add is ignored
  await userEvent.click(screen.getByRole('button', { name: 'Add' }))
  expect(onAdd).toHaveBeenCalledTimes(1)
})

test('Watchlist shows empty state', () => {
  render(<Watchlist items={[]} onSelect={vi.fn()} onAdd={vi.fn()} onRemove={vi.fn()} />)
  expect(screen.getByText(/no tickers yet/i)).toBeInTheDocument()
})

test('ThemeToggle shows the opposite theme and toggles', async () => {
  const onToggle = vi.fn()
  const { rerender } = render(<ThemeToggle theme="light" onToggle={onToggle} />)
  const btn = screen.getByRole('button')
  expect(btn).toHaveTextContent(/dark/i)
  await userEvent.click(btn)
  expect(onToggle).toHaveBeenCalled()
  rerender(<ThemeToggle theme="dark" onToggle={onToggle} />)
  expect(screen.getByRole('button')).toHaveTextContent(/light/i)
})

test('Skeleton renders a status label', () => {
  render(<Skeleton label="Loading prices" />)
  expect(screen.getByRole('status')).toHaveTextContent('Loading prices')
  render(<Skeleton />)
  expect(screen.getAllByRole('status')[1]).toHaveTextContent('Loading…')
})

test('Compare builds a metric matrix and dashes missing cells', () => {
  const { rerender } = render(
    <Compare
      rows={[
        { ticker: 'AAPL', metrics: { roe: 1.5, net_margin: 0.25 } },
        { ticker: 'MSFT', metrics: { roe: 0.4 } },
      ]}
    />,
  )
  expect(screen.getByText('1.50')).toBeInTheDocument()
  expect(screen.getByText('—')).toBeInTheDocument() // MSFT net_margin missing
  rerender(<Compare rows={[]} />)
  expect(screen.getByText(/nothing to compare/i)).toBeInTheDocument()
})
