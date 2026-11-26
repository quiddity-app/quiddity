import { expect, test } from './fixtures';

const requests = (page: import('@playwright/test').Page) =>
  page.evaluate(() => window.__quiddityRemote?.graphRequests ?? []);

test('without several graphs, the name is plain text', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('link', { name: 'Journals' })).toBeVisible();
  await expect(page.getByRole('button', { name: /^Graph:/ })).toHaveCount(0);
});

test('the corner names the open graph, and its menu lists the graphs and ways to make one', async ({
  page,
}) => {
  await page.goto('/?graphs');
  await page.getByRole('button', { name: 'Graph: Quiddity' }).first().click();
  const menu = page.getByRole('menu', { name: 'Graphs' });
  await expect(
    menu.getByRole('menuitemradio', { name: 'Quiddity' }),
  ).toBeChecked();
  await expect(
    menu.getByRole('menuitem', { name: 'New empty graph…' }),
  ).toBeVisible();
  await expect(
    menu.getByRole('menuitem', { name: 'New graph to sync…' }),
  ).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(menu).toHaveCount(0);
});

test('a new graph is asked for by name, opened, and listed from then on', async ({
  page,
}) => {
  await page.goto('/?graphs');
  await page.getByRole('button', { name: 'Graph: Quiddity' }).first().click();
  await page.getByRole('menuitem', { name: 'New graph to sync…' }).click();
  const dialog = page.getByRole('dialog', { name: 'New graph to sync' });
  await dialog.getByLabel('Name').fill('Work');
  await dialog.getByRole('button', { name: 'Create' }).click();
  await expect(
    page.getByRole('button', { name: 'Graph: Work' }).first(),
  ).toBeVisible();
  expect(await requests(page)).toEqual([]); // the page has started over
  await page.getByRole('button', { name: 'Graph: Work' }).first().click();
  await expect(page.getByRole('menuitemradio')).toHaveText([
    'Quiddity',
    'Work',
  ]);
});

test('a name the shell refuses is said, and nothing is made', async ({
  page,
}) => {
  await page.goto('/?graphs');
  await page.getByRole('button', { name: 'Graph: Quiddity' }).first().click();
  await page.getByRole('menuitem', { name: 'New empty graph…' }).click();
  const dialog = page.getByRole('dialog', { name: 'New empty graph' });
  await dialog.getByLabel('Name').fill('quiddity');
  await dialog.getByRole('button', { name: 'Create' }).click();
  await expect(dialog.getByRole('alert')).toContainText('already a graph');
  await expect(
    page.getByRole('button', { name: 'Graph: Quiddity' }).first(),
  ).toBeVisible();
  expect(await requests(page)).toEqual([]);
});

test('choosing another graph opens it', async ({ page }) => {
  await page.goto('/?graphs');
  await page.getByRole('button', { name: 'Graph: Quiddity' }).first().click();
  await page.getByRole('menuitem', { name: 'New empty graph…' }).click();
  await page.getByLabel('Name').fill('Home');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(
    page.getByRole('button', { name: 'Graph: Home' }).first(),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Graph: Home' }).first().click();
  await page.getByRole('menuitemradio', { name: 'Quiddity' }).click();
  await expect(
    page.getByRole('button', { name: 'Graph: Quiddity' }).first(),
  ).toBeVisible();
});

test('Escape closes the name dialog without making anything', async ({
  page,
}) => {
  await page.goto('/?graphs');
  await page.getByRole('button', { name: 'Graph: Quiddity' }).first().click();
  await page.getByRole('menuitem', { name: 'New empty graph…' }).click();
  await expect(page.getByLabel('Name')).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await requests(page)).toEqual([]);
});
