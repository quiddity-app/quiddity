import { expect, test, openEntity } from './fixtures';

/** Writes two links to a name nobody has made into a block of Quiddity. */
async function linkToNowhere(page: import('@playwright/test').Page) {
  await openEntity(page, 'Quiddity');
  await page.locator('main article > ul').getByText('Local-first').click();
  await page.keyboard.press('End');
  await page.keyboard.type(' and [[Fresh]] and [[Fresh]]');
  await page.keyboard.press('Escape');
}

async function openStats(page: import('@playwright/test').Page) {
  await page.getByRole('button', { name: 'More' }).click();
  await page
    .getByRole('menuitem', { name: 'Stats and debug', exact: true })
    .click();
  await expect(
    page.getByRole('heading', { name: 'Stats and debug', level: 1 }),
  ).toBeVisible();
}

test('the Stats and debug page is in the More menu and counts the graph', async ({
  page,
}) => {
  await page.goto('/');
  await openStats(page);
  const numbers = page.getByRole('region', { name: 'The graph' });
  for (const label of [
    'Blocks',
    'Entities',
    'Journal pages',
    'Links to entities',
    'Links to nowhere',
  ]) {
    await expect(numbers.getByText(label, { exact: true })).toBeVisible();
  }
  // The made-up graph has several entities, and journals among them.
  const entities = await numbers
    .getByText('Entities', { exact: true })
    .locator('xpath=following-sibling::dd[1]')
    .textContent();
  expect(Number(entities)).toBeGreaterThan(5);
});

test('a virtual entity is listed with its ID and how many links it has', async ({
  page,
}) => {
  await page.goto('/');
  await linkToNowhere(page);
  await openStats(page);

  const row = page.getByRole('row', { name: /^Fresh/ });
  await expect(row.getByRole('rowheader')).toHaveText('Fresh');
  // ID, incoming links, entities they are in, and uses as a key.
  await expect(row.getByRole('cell')).toHaveText(['~fresh', '2', '1', '0']);

  // It is a link to the entity's page, which is empty because nothing is stored.
  await row.getByRole('link', { name: 'Fresh' }).click();
  await expect(
    page.getByRole('heading', { name: 'Fresh', level: 1 }),
  ).toBeVisible();
});

test('a property key nobody has made is a virtual entity used as a key', async ({
  page,
}) => {
  await page.goto('/stats/');
  // The made-up graph's CRDT page writes `merges::`, which no entity defines.
  const row = page.getByRole('row', { name: /^merges/ });
  await expect(row.getByRole('cell')).toHaveText(['~merges', '0', '0', '1']);
});

test('the consistency check runs when asked, and says when it is out of date', async ({
  page,
}) => {
  await page.goto('/stats/');
  await expect(page.getByRole('status')).toHaveCount(0);
  await page.getByRole('button', { name: 'Run check' }).click();
  await expect(page.getByRole('status')).toHaveText(
    'The graph agrees with itself.',
  );
  const integrity = page
    .getByRole('heading', { name: 'Integrity' })
    .locator('xpath=..');
  await expect(integrity.getByText('Passed', { exact: false })).toHaveCount(5);
  await expect(
    page.getByText('The graph has changed since this ran.'),
  ).toHaveCount(0);

  // Another device makes an entity: what was found is about the graph as it was.
  await page.evaluate(() => window.__quiddityRemote?.createEntity('Elsewhere'));
  await expect(
    page.getByText('The graph has changed since this ran.'),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Run again' }).click();
  await expect(
    page.getByText('The graph has changed since this ran.'),
  ).toHaveCount(0);
});

test('the Sync section says how this device stands with the others', async ({
  page,
}) => {
  await page.goto('/');
  await openStats(page);
  const sync = page.getByRole('region', { name: 'Sync' });
  await expect(
    sync.getByText('Alone: No other devices to sync with'),
  ).toBeVisible();

  await page.evaluate(() => {
    window.__quiddityRemote!.setStatus('behind');
  });
  await expect(
    sync.getByText('Behind: Connected, but out of date'),
  ).toBeVisible();
  await expect(
    sync.getByText(/lacks, or typing is being held back/),
  ).toBeVisible();

  await page.evaluate(() => {
    window.__quiddityRemote!.setStatus('synced', {
      listed: 2,
      connected: [{ device: 'b'.repeat(64), ahead: false }],
    });
  });
  await expect(sync.getByText('bbbbbbbb…')).toBeVisible();
});

test('the Sync section shows this device and what the network has lately done', async ({
  page,
}) => {
  await page.goto('/');
  await openStats(page);
  const sync = page.getByRole('region', { name: 'Sync' });
  await expect(sync.getByText('Nothing has happened yet.')).toBeVisible();

  const refused = 'c'.repeat(64);
  await page.evaluate((device) => {
    window.__quiddityRemote!.setStatus('offline', {
      device: 'd'.repeat(64),
      log: [
        { at: 1_700_000_000_000, level: 'info', device, message: 'dialling' },
        {
          at: 1_700_000_001_000,
          level: 'problem',
          device,
          message: 'the peer holds another graph',
        },
      ],
    });
  }, refused);

  await expect(sync.getByText('d'.repeat(64))).toBeVisible();
  const entries = sync.getByRole('list', { name: 'Network log' }).locator('li');
  // Newest first, and a problem is marked in words as well as colour.
  await expect(entries).toHaveCount(2);
  await expect(entries.first()).toHaveAttribute('data-level', 'problem');
  await expect(entries.first()).toContainText(
    'Problem: cccccccc… the peer holds another graph',
  );
  await expect(entries.last()).toContainText('cccccccc… dialling');
});
