import { expect, test } from './fixtures';

test('a device that has a graph shows neither the welcome nor the wait', async ({
  page,
}) => {
  await page.goto('/');
  await expect(
    page.getByRole('link', { name: 'Journals', exact: true }),
  ).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('a device with no graph asks whether to start one or join, and joining is first', async ({
  page,
}) => {
  await page.goto('/?firstrun');
  const dialog = page.getByRole('dialog', { name: 'Welcome to Quiddity' });
  await expect(dialog).toBeVisible();
  // The choice that can't make a graph that won't merge is the one a stray
  // Enter takes.
  await expect(
    dialog.getByRole('button', { name: 'Join a graph' }),
  ).toBeFocused();
  await expect(dialog).toContainText(
    'will not merge with one on another device',
  );
});

test('each choice is what the shell is asked for', async ({ page }) => {
  await page.goto('/?firstrun=stubborn');
  for (const [button, how] of [
    ['Join a graph', 'join'],
    ['Start a new graph', 'new'],
  ]) {
    await page.getByRole('button', { name: button }).click();
    await expect(page.getByRole('dialog').getByRole('alert')).toHaveText(
      'permission denied',
    );
    expect(
      await page.evaluate(() => window.__quiddityRemote?.started.at(-1)),
    ).toBe(how);
  }
});

test('starting a new graph goes on with nothing in the way', async ({
  page,
}) => {
  await page.goto('/?firstrun');
  await page.getByRole('button', { name: 'Start a new graph' }).click();
  await expect(
    page.getByRole('link', { name: 'Journals', exact: true }),
  ).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(page.url()).not.toContain('firstrun');
});

test('joining waits for a graph, shows how to let the other device in, and goes when one arrives', async ({
  page,
}) => {
  await page.goto('/?firstrun');
  await page.getByRole('button', { name: 'Join a graph' }).click();

  const waiting = page.getByRole('dialog', { name: 'Waiting for a graph' });
  await expect(waiting).toBeVisible();
  // This device's ID, for the other to add, and a way to add the other's.
  await expect(waiting.getByText('f00d'.repeat(16))).toBeVisible();
  await expect(waiting.getByText('What this device is doing')).toBeVisible();
  await expect(
    waiting.getByText('No other devices to sync with'),
  ).toBeVisible();

  const other = 'a'.repeat(64);
  await waiting.getByLabel('Device ID').fill(other);
  await waiting.getByRole('button', { name: 'Add device' }).click();
  await expect(waiting.getByRole('status')).toContainText('Added.');
  expect(await page.evaluate(() => window.__quiddityRemote?.allowed)).toEqual([
    other,
  ]);

  // The graph arrives from the device that has it.
  await page.evaluate(() => {
    window.__quiddityRemote!.createEntity('From another device');
  });
  await expect(waiting).toHaveCount(0);
  // The app is there behind it.
  await expect(
    page.getByRole('link', { name: 'Journals', exact: true }),
  ).toBeVisible();
});

test('the network log is shown while waiting, so a problem is visible', async ({
  page,
}) => {
  await page.goto('/?blank&joining');
  const waiting = page.getByRole('dialog', { name: 'Waiting for a graph' });
  await expect(waiting).toBeVisible();
  await page.evaluate(() => {
    window.__quiddityRemote!.setStatus('offline', {
      log: [
        {
          at: 1_700_000_000_000,
          level: 'problem',
          device: 'c'.repeat(64),
          message: 'refused a connection',
        },
      ],
    });
  });
  await expect(
    waiting.getByRole('list', { name: 'Network log' }),
  ).toContainText('Problem: cccccccc… refused a connection');
});
