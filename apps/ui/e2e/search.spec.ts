import { expect, test, openSearch } from './fixtures';

test('Ctrl-K opens search, and fuzzy title search opens a page', async ({
  page,
}) => {
  await page.goto('/');
  await openSearch(page);
  const panel = page.getByRole('dialog', { name: 'Search' });
  await expect(panel.getByRole('combobox')).toBeFocused();
  await panel.getByRole('combobox').fill('rdng');
  const pages = panel.getByRole('group', { name: 'Pages' });
  await expect(pages.getByRole('option').first()).toHaveText('Reading list');
  await page.keyboard.press('Enter');
  await expect(panel).toHaveCount(0);
  await expect(
    page.getByRole('heading', { name: 'Reading list', level: 1 }),
  ).toBeVisible();
});

test('capitals match the first letters of words', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /^Search/ }).click();
  const box = page.getByRole('combobox');
  await box.fill('RL');
  await expect(
    page.getByRole('option', { name: 'Reading list' }),
  ).toBeVisible();
  await box.fill('Rl');
  await expect(
    page.getByRole('option', { name: 'Reading list' }),
  ).toBeVisible();
  await box.fill('LR');
  await expect(page.getByText('Nothing found.')).toBeVisible();
});

test('arrows choose, Shift-Enter opens at the side, Escape closes', async ({
  page,
}) => {
  await page.goto('/');
  await openSearch(page);
  const box = page.getByRole('combobox');
  await box.fill('d');
  const options = page.getByRole('option');
  // Wait for the list to be the answer to "d": before that it is the answer to
  // nothing, and the entry read below could be replaced before it is chosen.
  await expect(options.first().locator('mark').first()).toBeVisible();
  await expect(options.first()).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('ArrowDown');
  await expect(options.nth(1)).toHaveAttribute('aria-selected', 'true');
  const name = (await options.nth(1).textContent()) ?? '';
  await page.keyboard.press('Shift+Enter');
  await expect(
    page.locator('aside article').getByRole('heading', { name }),
  ).toBeVisible();

  await openSearch(page);
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('text search finds blocks by their words, and opens the page they are on', async ({
  page,
}) => {
  await page.goto('/');
  await openSearch(page);
  // Order doesn't matter, and the last word may be unfinished.
  await page.getByRole('combobox').fill('crdts note');
  const text = page.getByRole('group', { name: 'Text' });
  const hit = text.getByRole('option').first();
  await expect(hit).toContainText('Notes on loro and other CRDTs');
  await expect(hit.locator('mark')).toHaveText(['Notes', 'CRDTs']);
  await hit.click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(
    'Reading list',
  );
});

test('a result chosen before / has redirected is not overwritten by the redirect', async ({
  page,
}) => {
  // Choose the first text result the moment it is drawn, which is when the
  // graph has loaded and before the redirect has run, and make the page it
  // leads to slow, so that the navigation is still under way when the
  // redirect fires. Otherwise the race is there, but too narrow to hit.
  await page.route('**/entity/**', async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 1500));
    await route.continue();
  });
  await page.addInitScript(() => {
    const watch = new MutationObserver(() => {
      const hit = document.querySelector<HTMLElement>(
        '[role="group"][aria-label="Text"] [role="option"]',
      );
      if (hit) {
        watch.disconnect();
        hit.click();
      }
    });
    watch.observe(document, { childList: true, subtree: true });
  });
  await page.goto('/');
  await openSearch(page);
  await page.getByRole('combobox').fill('crdts note');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText(
    'Reading list',
  );
  await expect(page).toHaveURL(/\/entity\//);
});

test('a word that is nowhere finds nothing', async ({ page }) => {
  await page.goto('/');
  await openSearch(page);
  await page.getByRole('combobox').fill('loro zzzz');
  await expect(page.getByText('Nothing found.')).toBeVisible();
});
