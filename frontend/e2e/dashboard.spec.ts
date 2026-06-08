import { test, expect } from '@playwright/test'

// Mocks the backend so the full auth -> watchlist -> company flow runs offline.
test('log in, pick a watchlist ticker, see its data', async ({ page }) => {
  await page.route('**/auth/login', (route) => route.fulfill({ json: { token: 'test-token' } }))
  await page.route('**/api/watchlist', (route) =>
    route.fulfill({
      json: [{ id: 1, cik: '', ticker: 'AAPL', name: 'Apple Inc.', exchange: 'NASDAQ', sector: null, industry: null }],
    }),
  )
  const base = '**/api/companies/AAPL'
  await page.route(base, (route) =>
    route.fulfill({ json: { id: 1, cik: '', ticker: 'AAPL', name: 'Apple Inc.', exchange: 'NASDAQ', sector: null, industry: null } }),
  )
  await page.route(`${base}/prices`, (route) =>
    route.fulfill({ json: [{ company_id: 1, date: '2024-01-02', close: 185, volume: 1, source: 'fmp' }] }),
  )
  await page.route(`${base}/facts`, (route) =>
    route.fulfill({
      json: [{ company_id: 1, statement: 'income', line_item: 'Revenue', period_type: 'annual', period_end: '2023-12-31', value: 383285000000, source: 'edgar', fetched_at: '2024-01-01T00:00:00Z' }],
    }),
  )
  await page.route(`${base}/ratios`, (route) => route.fulfill({ json: [] }))
  await page.route(`${base}/news`, (route) => route.fulfill({ json: [] }))
  await page.route(`${base}/discrepancies`, (route) => route.fulfill({ json: [] }))
  await page.route(`${base}/graham`, (route) =>
    route.fulfill({
      json: {
        criteria: [{ name: 'Current ratio >= 2', passed: true, detail: 'current ratio 2.5' }],
        score: 1,
        graham_number: 22.4,
        ncav_per_share: null,
        margin_of_safety: 0.1,
        net_net: false,
        passes_defensive: true,
      },
    }),
  )

  await page.goto('/')
  await page.getByLabel('email').fill('a@e.com')
  await page.getByLabel('password').fill('pw')
  await page.getByRole('button', { name: /log in/i }).click()

  // dashboard + watchlist (exact: the "remove AAPL" button also contains AAPL)
  const pick = page.getByRole('button', { name: 'AAPL', exact: true })
  await expect(pick).toBeVisible()
  await pick.click()

  await expect(page.getByRole('heading', { name: /apple inc/i })).toBeVisible()
  await expect(page.getByText('Revenue')).toBeVisible()
  await expect(page.getByText('$383.3B')).toBeVisible()
  await expect(page.getByText(/graham scorecard/i)).toBeVisible()
})
