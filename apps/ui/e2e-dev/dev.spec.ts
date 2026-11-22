import { expect, test } from '../e2e/fixtures';

// The other browser tests run against the static export, which never renders
// these pages on a server. `next dev` does, on every request, so anything that
// can't run there (such as the graph, which only exists in the browser) fails
// here and only here. This is what `mise run app` and `pnpm ui dev` run.

test('the dev server renders the app, and the graph loads in the browser', async ({
  page,
}) => {
  await page.goto('/');
  await expect(
    page.getByRole('heading', { name: 'Journals', level: 1 }),
  ).toBeVisible();
  // Favourites come from the graph, so the sidebar has really loaded it.
  await expect(
    page.locator('nav').getByRole('link', { name: 'Quiddity' }),
  ).toBeVisible();
});

test('an entity opens and can be edited in the dev server too', async ({
  page,
}) => {
  await page.goto('/all/');
  await page
    .locator('main')
    .getByRole('link', { name: 'Reading list', exact: true })
    .click();
  await page.locator('main article > ul [data-block-text]').first().click();
  await page.keyboard.type('typed in dev');
  await page.keyboard.press('Escape');
  await expect(page.getByText('typed in dev')).toBeVisible();
});
