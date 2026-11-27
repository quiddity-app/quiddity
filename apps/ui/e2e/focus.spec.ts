import { clickAway, editBlock, expect, openEntity, test } from './fixtures';

const block = 'A CRDT library. Documents are movable trees of text.';

test('the block being edited is reported, and stops being when editing does', async ({
  page,
}) => {
  await openEntity(page, 'Loro');
  await editBlock(page, block);
  const focus = () =>
    page.evaluate(() => window.__quiddityRemote?.focus.length);
  await expect.poll(focus).toBe(1);
  await clickAway(page);
  await expect.poll(focus).toBe(0);
});

test('moving to another block is told as one move', async ({ page }) => {
  await openEntity(page, 'Quiddity');
  await editBlock(page, 'Local-first, multiplayer second brain.');
  const focus = () =>
    page.evaluate(() => window.__quiddityRemote?.focus.join(',') ?? '');
  await expect.poll(focus).toMatch(/^[^,]+$/);
  const first = await focus();
  await clickAway(page);
  await editBlock(page, 'Open questions');
  await expect.poll(focus).toMatch(/^[^,]+$/);
  expect(await focus()).not.toBe(first);
});

test('typing is pushed as typing, and anything else as itself', async ({
  page,
}) => {
  await openEntity(page, 'Loro');
  await editBlock(page, block);
  await page.keyboard.press('End');
  await page.keyboard.type(' Typed.');
  const pushes = () =>
    page.evaluate(() => window.__quiddityRemote?.typed ?? []);
  await expect.poll(async () => (await pushes()).at(-1)?.length).toBe(1);

  // Splitting the block is structure, so it isn't held back.
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await pushes()).at(-1)?.length).toBe(0);
  expect((await pushes()).length).toBeGreaterThan(1);
});

test('typing that never pauses is still pushed as it goes', async ({
  page,
}) => {
  await openEntity(page, 'Loro');
  await editBlock(page, block);
  await page.keyboard.press('End');
  // Pushes of typing in one block: the page's own start-up ones are structure.
  const pushes = () =>
    page.evaluate(
      () =>
        window.__quiddityRemote?.typed.filter((t) => t.length === 1).length ??
        0,
    );
  const before = await pushes();
  // A key every 40 ms for three seconds is never the 300 ms of quiet a plain
  // debounce waits for. Read before the trailing push is due, so what is
  // counted was sent while the typing was still going on.
  await page.keyboard.type('x'.repeat(75), { delay: 40 });
  expect((await pushes()) - before).toBeGreaterThanOrEqual(2);
});

test('a hidden page has no block open, and has it again when shown', async ({
  page,
}) => {
  await openEntity(page, 'Loro');
  await editBlock(page, block);
  await page.keyboard.press('End');
  await page.keyboard.type(' Typed.');
  const focus = () =>
    page.evaluate(() => window.__quiddityRemote?.focus.length);
  await expect.poll(focus).toBe(1);

  // A phone's screen going off: the shell can't release typing it is holding
  // back once it is suspended, so the page lets go of the block first.
  const setHidden = (hidden: boolean) =>
    page.evaluate((hidden) => {
      Object.defineProperty(document, 'hidden', {
        configurable: true,
        value: hidden,
      });
      document.dispatchEvent(new Event('visibilitychange'));
    }, hidden);
  await setHidden(true);
  await expect.poll(focus).toBe(0);
  // What was typed reached the shell before it was told, as for any move.
  const last = () =>
    page.evaluate(() => window.__quiddityRemote?.typed.at(-1)?.length);
  expect(await last()).toBe(1);

  // Going to sleep says nothing of the network; waking up does, since the
  // sockets and relay connection of a phone don't survive being asleep.
  const networkChanges = () =>
    page.evaluate(() => window.__quiddityRemote?.networkChanges);
  expect(await networkChanges()).toBe(0);

  await setHidden(false);
  await expect.poll(focus).toBe(1);
  await expect.poll(networkChanges).toBe(1);
});

test('leaving a block that was only typed in is still told as typing', async ({
  page,
}) => {
  await openEntity(page, 'Loro');
  await editBlock(page, block);
  await page.keyboard.press('End');
  await page.keyboard.type(' Typed.');
  await clickAway(page);
  const focus = () =>
    page.evaluate(() => window.__quiddityRemote?.focus.length);
  await expect.poll(focus).toBe(0);
  // Saving the block on leaving it changed nothing, so it isn't structure that
  // the shell must not hold back.
  const last = () =>
    page.evaluate(() => window.__quiddityRemote?.typed.at(-1)?.length);
  expect(await last()).toBe(1);
});
