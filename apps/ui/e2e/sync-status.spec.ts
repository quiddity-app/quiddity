import { expect, test } from './fixtures';

const states = [
  ['alone', 'No other devices to sync with'],
  ['offline', 'Not connected to any other device'],
  ['behind', 'Connected, but out of date'],
  ['synced', 'Connected and up to date'],
  ['present', 'Another device is here'],
] as const;

test('the sync indicator sits left of search, and starts with nobody to sync with', async ({
  page,
}) => {
  await page.goto('/');
  const indicator = page.getByRole('button', { name: /^Sync:/ });
  await expect(indicator).toHaveAttribute('data-state', 'alone');
  await expect(indicator).toHaveAccessibleName(
    'Sync: No other devices to sync with',
  );

  const here = await indicator.boundingBox();
  const search = await page
    .getByRole('button', { name: /^Search/ })
    .boundingBox();
  expect(here!.x + here!.width).toBeLessThanOrEqual(search!.x);
});

test('it shows each state the shell can report', async ({ page }) => {
  await page.goto('/');
  const indicator = page.getByRole('button', { name: /^Sync:/ });
  await expect(indicator).toBeVisible();
  for (const [state, label] of states) {
    await page.evaluate((state) => {
      window.__quiddityRemote!.setStatus(state);
    }, state);
    await expect(indicator).toHaveAttribute('data-state', state);
    await expect(indicator).toHaveAccessibleName(`Sync: ${label}`);
  }
});

test('each state has a colour of its own', async ({ page }) => {
  await page.goto('/');
  const indicator = page.getByRole('button', { name: /^Sync:/ });
  await expect(indicator).toBeVisible();
  const colours: string[] = [];
  for (const [state] of states) {
    await page.evaluate((state) => {
      window.__quiddityRemote!.setStatus(state);
    }, state);
    await expect(indicator).toHaveAttribute('data-state', state);
    colours.push(
      await indicator
        .locator('span')
        .evaluate((dot) => getComputedStyle(dot).backgroundColor),
    );
  }
  expect(new Set(colours).size).toBe(states.length);
});

test('pressing it shows the details, and a way to the Devices page', async ({
  page,
}) => {
  await page.goto('/');
  const indicator = page.getByRole('button', { name: /^Sync:/ });
  await expect(indicator).toHaveAttribute('aria-expanded', 'false');
  await indicator.click();
  await expect(indicator).toHaveAttribute('aria-expanded', 'true');

  const details = page.getByRole('dialog', { name: 'Sync details' });
  await expect(details).toContainText('No other devices to sync with');
  await expect(details.getByText('Connected', { exact: true })).toBeVisible();

  await details.getByRole('link', { name: 'Devices' }).click();
  await expect(
    page.getByRole('heading', { name: 'Devices', level: 1 }),
  ).toBeVisible();
  await expect(details).toBeHidden();
});

test('the details name who is connected and what is held back', async ({
  page,
}) => {
  await page.goto('/');
  const indicator = page.getByRole('button', { name: /^Sync:/ });
  await expect(indicator).toBeVisible();
  await page.evaluate(() => {
    window.__quiddityRemote!.setStatus('behind', {
      listed: 3,
      connected: [{ device: 'a'.repeat(64), ahead: true }],
      held: 2,
      watching: 1,
    });
  });
  await indicator.click();
  const details = page.getByRole('dialog', { name: 'Sync details' });
  const value = (term: string) =>
    details
      .getByText(term, { exact: true })
      .locator('xpath=following-sibling::dd[1]');
  await expect(value('Connected')).toContainText('aaaaaaaa…');
  await expect(value('Connected')).toContainText('has more than this');
  await expect(value('Connected').locator('code')).toHaveAttribute(
    'title',
    'a'.repeat(64),
  );
  await expect(value('Listed')).toHaveText('3');
  await expect(value('Typing held back')).toHaveText('2');
  await expect(value('Here now')).toHaveText('1');

  // It keeps up while it is open.
  await page.evaluate(() => {
    window.__quiddityRemote!.setStatus('synced', { listed: 3 });
  });
  await expect(value('Connected')).toHaveText('None');
  await expect(details).toContainText('Connected and up to date');
});

test('Escape and a press elsewhere close the details', async ({ page }) => {
  await page.goto('/');
  const indicator = page.getByRole('button', { name: /^Sync:/ });
  const details = page.getByRole('dialog', { name: 'Sync details' });
  await indicator.click();
  await expect(details).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(details).toBeHidden();
  await expect(indicator).toBeFocused();

  await indicator.click();
  await expect(details).toBeVisible();
  await page.getByRole('heading', { level: 1 }).first().click({ force: true });
  await expect(details).toBeHidden();
});
