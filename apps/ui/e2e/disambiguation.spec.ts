import { expect, test, openEntity } from './fixtures';

const block = (page: import('@playwright/test').Page) =>
  page.locator('main article > ul').getByText('Local-first');

test.beforeEach(async ({ page }) => {
  await openEntity(page, 'Quiddity');
});

test('an ambiguous link opens a page listing what it matches, with their properties', async ({
  page,
}) => {
  await block(page).click();
  await page.keyboard.press('End');
  // Qualified links make the two entities; the bare one then matches both.
  await page.keyboard.type(
    ' [[Mercury #type(planet)]] [[Mercury #type(metal)]] [[Mercury]]',
  );
  await page.keyboard.press('Escape');

  await page
    .locator('main article > ul')
    .getByRole('link', { name: 'Mercury' })
    .last()
    .click();
  await expect(
    page.getByRole('heading', { name: /Which/, level: 1 }),
  ).toContainText('Mercury');
  const candidates = page.locator('[data-candidate]');
  await expect(candidates).toHaveCount(2);
  await expect(candidates.filter({ hasText: 'planet' })).toHaveCount(1);
  await expect(candidates.filter({ hasText: 'metal' })).toHaveCount(1);

  await candidates
    .filter({ hasText: 'metal' })
    .getByRole('link', { name: 'Mercury' })
    .click();
  await expect(
    page.getByRole('heading', { name: 'Mercury', level: 1 }),
  ).toBeVisible();
});
