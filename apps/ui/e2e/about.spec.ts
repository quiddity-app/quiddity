import { expect, test } from './fixtures';

test('About is the last item in the More menu, and shows what Quiddity is built from', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'More' }).click();
  const items = page.getByRole('menuitem');
  await expect(items.last()).toHaveText('About');
  await items.last().click();

  await expect(
    page.getByRole('heading', { name: 'About Quiddity', level: 1 }),
  ).toBeVisible();
  await expect(page.getByText('Apache License 2.0')).toBeVisible();
  await expect(page.getByRole('menu')).toHaveCount(0);

  // The list is what `scripts/licenses.mjs` wrote when the UI was built.
  const count = page.getByText(/^\d+ of \d+ packages$/);
  await expect(count).toBeVisible();
  const total = Number(/of (\d+)/.exec((await count.textContent()) ?? '')?.[1]);
  expect(total).toBeGreaterThan(100);

  // Filtering narrows it by name, and then the text of one can be read.
  await page.getByLabel('Filter').fill('serde_json');
  await expect(count).toHaveText(
    new RegExp(String.raw`^\d+ of ${total} packages$`),
  );
  const row = page.getByRole('button', { name: /^serde_json\b/ }).first();
  await expect(row).toHaveAttribute('aria-expanded', 'false');
  await row.click();
  await expect(row).toHaveAttribute('aria-expanded', 'true');
  await expect(page.locator('pre').first()).toContainText(/\S/);
});

test('a filter that matches nothing says so', async ({ page }) => {
  await page.goto('/about/');
  await page.getByLabel('Filter').fill('no such package anywhere');
  await expect(page.getByText(/^0 of \d+ packages$/)).toBeVisible();
});
