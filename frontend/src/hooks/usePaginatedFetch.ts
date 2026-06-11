import { useEffect, useState } from 'react'
import type { DependencyList } from 'react'
import type { Page } from '../types'

export interface PaginatedState<T> {
  rows: T[]
  total: number
  loading: boolean
  error: string | null
}

/**
 * Run a paginated fetch whenever `deps` change. Tracks `loading`/`error` and
 * ignores stale responses, so a slow earlier request can never overwrite a
 * newer one and no state is set after unmount. `fetcher` should close over the
 * current query; `deps` are the values that re-trigger it (same contract as a
 * `useEffect` dependency array).
 */
export function usePaginatedFetch<T>(
  fetcher: () => Promise<Page<T>>,
  deps: DependencyList,
): PaginatedState<T> {
  const [state, setState] = useState<PaginatedState<T>>({
    rows: [],
    total: 0,
    loading: true,
    error: null,
  })

  useEffect(() => {
    let active = true
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setState((s) => ({ ...s, loading: true, error: null }))
    fetcher()
      .then((page) => {
        if (active) {
          setState({ rows: page.rows, total: page.total, loading: false, error: null })
        }
      })
      .catch((e: unknown) => {
        if (active) {
          setState((s) => ({
            ...s,
            loading: false,
            error: e instanceof Error ? e.message : String(e),
          }))
        }
      })
    return () => {
      active = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps)

  return state
}
