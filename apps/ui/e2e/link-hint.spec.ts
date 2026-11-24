import { expect, test, openEntity } from './fixtures';

const status = (page: import('@playwright/test').Page) =>
  page.locator('[data-link-status]');

test.beforeEach(async ({ page }) => {
  await openEntity(page, 'Quiddity');
});

test('hovering an entity link names the entity in the corner, and leaving clears it', async ({
  page,
}) => {
  const link = page
    .locator('main article')
    .getByRole('link', { name: 'Loro', exact: true });
  await link.hover();
  await expect(status(page)).toHaveText('Loro');
  await page.mouse.move(0, 400);
  await expect(status(page)).toHaveCount(0);
});

test('focusing a link says the same as hovering it', async ({ page }) => {
  const link = page
    .locator('main article')
    .getByRole('link', { name: 'iroh', exact: true });
  await link.focus();
  await expect(status(page)).toHaveText('iroh');
  await link.blur();
  await expect(status(page)).toHaveCount(0);
});

test('an entity with a properties block shows its title and properties on hover', async ({
  page,
}) => {
  const link = page
    .locator('main article')
    .getByRole('link', { name: 'Loro', exact: true });
  await link.hover();
  const preview = page.getByRole('tooltip');
  await expect(preview).toContainText('Loro');
  await expect(preview).toContainText('kind of');
  await expect(preview).toContainText('CRDT');
  await page.keyboard.press('Escape');
  await expect(preview).toHaveCount(0);
});

test('an entity with no properties block has the toast but no tooltip', async ({
  page,
}) => {
  const link = page
    .locator('main article')
    .getByRole('link', { name: 'iroh', exact: true });
  await link.hover();
  await expect(status(page)).toHaveText('iroh');
  // Longer than the preview's delay.
  await page.waitForTimeout(600);
  await expect(page.getByRole('tooltip')).toHaveCount(0);
});

test('an external link shows its URL, and none is fetched', async ({
  page,
}) => {
  await page.locator('main article > ul').getByText('Local-first').click();
  await page.keyboard.press('End');
  await page.keyboard.type(' [docs](https://example.com/a?b=c)');
  await page.keyboard.press('Escape');
  await page
    .locator('main article')
    .getByRole('link', { name: 'docs' })
    .hover();
  await expect(status(page)).toHaveText('https://example.com/a?b=c');
});

test("a link to an app page shows the page's name", async ({ page }) => {
  await page.locator('main article > ul').getByText('Local-first').click();
  await page.keyboard.press('End');
  await page.keyboard.type(' [stats](/stats/)');
  await page.keyboard.press('Escape');
  await page
    .locator('main article')
    .getByRole('link', { name: 'stats' })
    .hover();
  await expect(status(page)).toHaveText('Stats and debug');
});
