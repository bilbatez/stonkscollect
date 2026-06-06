import { test, expect } from '@playwright/test'

test('dashboard loads and shows the app title', async ({ page }) => {
  await page.goto('/')
  await expect(
    page.getByRole('heading', { name: /stonkscollect/i }),
  ).toBeVisible()
})
