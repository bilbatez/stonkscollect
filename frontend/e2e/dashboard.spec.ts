import { test, expect } from '@playwright/test'

// Mock the backend so the dashboard flow runs without a live API.
test('loads a company dashboard after entering a ticker', async ({ page }) => {
  const base = '**/api/companies/AAPL'
  await page.route(base, (route) =>
    route.fulfill({
      json: { id: 1, cik: '', ticker: 'AAPL', name: 'Apple Inc.', exchange: 'NASDAQ', sector: null, industry: null },
    }),
  )
  await page.route(`${base}/prices`, (route) =>
    route.fulfill({ json: [{ company_id: 1, date: '2024-01-02', close: 185, volume: 1, source: 'fmp' }] }),
  )
  await page.route(`${base}/facts`, (route) =>
    route.fulfill({
      json: [
        {
          company_id: 1,
          statement: 'income',
          line_item: 'Revenue',
          period_type: 'annual',
          period_end: '2023-12-31',
          value: 383285000000,
          source: 'edgar',
          fetched_at: '2024-01-01T00:00:00Z',
        },
      ],
    }),
  )
  await page.route(`${base}/ratios`, (route) => route.fulfill({ json: [] }))
  await page.route(`${base}/news`, (route) => route.fulfill({ json: [] }))
  await page.route(`${base}/discrepancies`, (route) => route.fulfill({ json: [] }))

  await page.goto('/')
  await page.getByLabel('ticker').fill('AAPL')
  await page.getByRole('button', { name: /load/i }).click()

  await expect(page.getByRole('heading', { name: /apple inc\./i })).toBeVisible()
  await expect(page.getByText('Revenue')).toBeVisible()
  await expect(page.getByText('$383.3B')).toBeVisible()
})
