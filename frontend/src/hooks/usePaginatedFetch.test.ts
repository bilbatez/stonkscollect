import { renderHook, waitFor } from '@testing-library/react'
import { describe, expect, test } from 'vitest'
import { usePaginatedFetch } from './usePaginatedFetch'
import type { Page } from '../types'

/** A promise plus its resolve/reject, so a test can settle the fetch on demand. */
function deferred<T>() {
  let resolve!: (v: T) => void
  let reject!: (e: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

const page: Page<number> = { rows: [1, 2], total: 9 }

describe('usePaginatedFetch', () => {
  test('starts loading, then exposes rows + total on success', async () => {
    const { result } = renderHook(() => usePaginatedFetch(() => Promise.resolve(page), []))
    expect(result.current.loading).toBe(true)
    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.rows).toEqual([1, 2])
    expect(result.current.total).toBe(9)
    expect(result.current.error).toBeNull()
  })

  test('surfaces an Error message', async () => {
    const { result } = renderHook(() =>
      usePaginatedFetch(() => Promise.reject(new Error('boom')), []),
    )
    await waitFor(() => expect(result.current.error).toBe('boom'))
    expect(result.current.loading).toBe(false)
  })

  test('stringifies a non-Error rejection', async () => {
    const { result } = renderHook(() =>
      usePaginatedFetch(() => Promise.reject('plain'), []),
    )
    await waitFor(() => expect(result.current.error).toBe('plain'))
  })

  test('ignores a response that resolves after unmount', async () => {
    const d = deferred<Page<number>>()
    const { result, unmount } = renderHook(() => usePaginatedFetch(() => d.promise, []))
    unmount()
    d.resolve(page)
    await Promise.resolve()
    // State frozen at its last pre-unmount value (still loading), not overwritten.
    expect(result.current.rows).toEqual([])
    expect(result.current.loading).toBe(true)
  })

  test('ignores a rejection that settles after unmount', async () => {
    const d = deferred<Page<number>>()
    const { result, unmount } = renderHook(() => usePaginatedFetch(() => d.promise, []))
    unmount()
    d.reject(new Error('late'))
    await Promise.resolve()
    expect(result.current.error).toBeNull()
  })
})
