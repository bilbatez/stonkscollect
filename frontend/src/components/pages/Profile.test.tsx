import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { Profile } from './Profile'
import * as api from '../../api'

vi.mock('../../api')
const mocked = vi.mocked(api)

beforeEach(() => {
  mocked.getMe.mockResolvedValue({ email: 'u@e.com', display_name: 'Uma' })
  mocked.updateProfile.mockResolvedValue()
  mocked.changePassword.mockResolvedValue()
})
afterEach(() => vi.clearAllMocks())

test('loads the profile and saves email + display name', async () => {
  const onProfileSaved = vi.fn()
  render(<Profile onProfileSaved={onProfileSaved} />)
  await waitFor(() => expect(screen.getByLabelText('profile email')).toHaveValue('u@e.com'))
  expect(screen.getByLabelText('display name')).toHaveValue('Uma')
  await userEvent.clear(screen.getByLabelText('profile email'))
  await userEvent.type(screen.getByLabelText('profile email'), 'new@e.com')
  await userEvent.clear(screen.getByLabelText('display name'))
  await userEvent.type(screen.getByLabelText('display name'), 'Umar')
  await userEvent.click(screen.getByRole('button', { name: /save profile/i }))
  await waitFor(() => expect(mocked.updateProfile).toHaveBeenCalledWith('new@e.com', 'Umar'))
  expect(await screen.findByText(/profile saved/i)).toBeInTheDocument()
  expect(onProfileSaved).toHaveBeenCalled()
})

test('surfaces a duplicate-email profile error', async () => {
  mocked.updateProfile.mockRejectedValue(new Error('email already registered'))
  render(<Profile />)
  await waitFor(() => expect(screen.getByLabelText('profile email')).toHaveValue('u@e.com'))
  await userEvent.click(screen.getByRole('button', { name: /save profile/i }))
  expect(await screen.findByText(/email already registered/i)).toBeInTheDocument()
})

test('profile non-Error rejection shows fallback message', async () => {
  mocked.updateProfile.mockRejectedValue('boom')
  render(<Profile />)
  await waitFor(() => expect(screen.getByLabelText('profile email')).toHaveValue('u@e.com'))
  await userEvent.click(screen.getByRole('button', { name: /save profile/i }))
  expect(await screen.findByText(/save failed/i)).toBeInTheDocument()
})

test('rejects mismatched new passwords without calling the API', async () => {
  render(<Profile />)
  await waitFor(() => expect(screen.getByLabelText('profile email')).toHaveValue('u@e.com'))
  await userEvent.type(screen.getByLabelText('current password'), 'old')
  await userEvent.type(screen.getByLabelText('new password'), 'aaaaaa')
  await userEvent.type(screen.getByLabelText('confirm password'), 'bbbbbb')
  await userEvent.click(screen.getByRole('button', { name: /change password/i }))
  expect(await screen.findByText(/do not match/i)).toBeInTheDocument()
  expect(mocked.changePassword).not.toHaveBeenCalled()
})

test('changes the password and clears the fields', async () => {
  render(<Profile />)
  await waitFor(() => expect(screen.getByLabelText('profile email')).toHaveValue('u@e.com'))
  await userEvent.type(screen.getByLabelText('current password'), 'old')
  await userEvent.type(screen.getByLabelText('new password'), 'newpass')
  await userEvent.type(screen.getByLabelText('confirm password'), 'newpass')
  await userEvent.click(screen.getByRole('button', { name: /change password/i }))
  await waitFor(() => expect(mocked.changePassword).toHaveBeenCalledWith('old', 'newpass'))
  expect(await screen.findByText(/password changed/i)).toBeInTheDocument()
  expect(screen.getByLabelText('current password')).toHaveValue('')
})

test('surfaces a wrong-old-password error', async () => {
  mocked.changePassword.mockRejectedValue(new Error('incorrect password'))
  render(<Profile />)
  await waitFor(() => expect(screen.getByLabelText('profile email')).toHaveValue('u@e.com'))
  await userEvent.type(screen.getByLabelText('current password'), 'wrong')
  await userEvent.type(screen.getByLabelText('new password'), 'newpass')
  await userEvent.type(screen.getByLabelText('confirm password'), 'newpass')
  await userEvent.click(screen.getByRole('button', { name: /change password/i }))
  expect(await screen.findByText(/incorrect password/i)).toBeInTheDocument()
})

test('password non-Error rejection shows fallback message', async () => {
  mocked.changePassword.mockRejectedValue('boom')
  render(<Profile />)
  await waitFor(() => expect(screen.getByLabelText('profile email')).toHaveValue('u@e.com'))
  await userEvent.type(screen.getByLabelText('current password'), 'old')
  await userEvent.type(screen.getByLabelText('new password'), 'newpass')
  await userEvent.type(screen.getByLabelText('confirm password'), 'newpass')
  await userEvent.click(screen.getByRole('button', { name: /change password/i }))
  expect(await screen.findByText(/change failed/i)).toBeInTheDocument()
})

test('saves with no onProfileSaved callback', async () => {
  render(<Profile />)
  await waitFor(() => expect(screen.getByLabelText('profile email')).toHaveValue('u@e.com'))
  await userEvent.click(screen.getByRole('button', { name: /save profile/i }))
  expect(await screen.findByText(/profile saved/i)).toBeInTheDocument()
})
