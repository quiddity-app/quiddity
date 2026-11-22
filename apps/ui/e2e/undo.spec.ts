import {
  expect,
  test,
  clickAway,
  editBlock,
  editor,
  mainBlocks,
  openEntity,
} from './fixtures';

const DDIA = 'Designing Data-Intensive Applications';
const NOTES = 'Notes on loro and other CRDTs';

test.beforeEach(async ({ page }) => {
  await openEntity(page, 'Reading list');
});

test('one undo takes back a whole burst of typing, and redo puts it back', async ({
  page,
}) => {
  await editBlock(page, DDIA);
  await page.keyboard.press('End');
  await page.keyboard.type(' 2e edition', { delay: 15 });
  await page.keyboard.press('Control+z');
  await expect(editor(page)).toHaveText(DDIA);
  await page.keyboard.press('Control+Shift+Z');
  await expect(editor(page)).toHaveText(`${DDIA} 2e edition`);
  await page.keyboard.press('Control+z');
  await page.keyboard.press('Control+y');
  await expect(editor(page)).toHaveText(`${DDIA} 2e edition`);
});

test('typing after a pause is a step of its own', async ({ page }) => {
  await editBlock(page, DDIA);
  await page.keyboard.press('End');
  await page.keyboard.type('one', { delay: 15 });
  await page.waitForTimeout(1000);
  await page.keyboard.type('two', { delay: 15 });
  await page.keyboard.press('Control+z');
  await expect(editor(page)).toHaveText(`${DDIA}one`);
  await page.keyboard.press('Control+z');
  await expect(editor(page)).toHaveText(DDIA);
});

test('a split undoes from inside the editor', async ({ page }) => {
  await editBlock(page, DDIA);
  await page.keyboard.press('Home');
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Enter');
  await expect.poll(() => mainBlocks(page)).toHaveLength(3);
  await page.keyboard.press('Control+z');
  await expect.poll(() => mainBlocks(page)).toEqual([DDIA, NOTES]);
});

test('with no editor open, undo and redo still work', async ({ page }) => {
  await editBlock(page, NOTES);
  await page.keyboard.press('Home');
  await page.keyboard.press('Backspace');
  await page.keyboard.press('Escape');
  await expect(editor(page)).toHaveCount(0);
  expect(await mainBlocks(page)).toEqual([DDIA + NOTES]);
  await clickAway(page);
  await page.keyboard.press('Control+z');
  await expect.poll(() => mainBlocks(page)).toEqual([DDIA, NOTES]);
  await page.keyboard.press('Control+y');
  await expect.poll(() => mainBlocks(page)).toEqual([DDIA + NOTES]);
});

test("undo is the graph's: it reaches an edit made on another page", async ({
  page,
}) => {
  await editBlock(page, DDIA);
  await page.keyboard.press('End');
  await page.keyboard.type(' [[iroh]]');
  await page.keyboard.press('Escape');
  await page
    .locator('main')
    .getByRole('link', { name: 'iroh', exact: true })
    .first()
    .click();
  await expect(
    page.getByRole('heading', { name: 'iroh', level: 1 }),
  ).toBeVisible();
  const readingList = page
    .getByRole('region', { name: 'Linked references' })
    .getByRole('heading', { name: 'Reading list' });
  await expect(readingList).toBeVisible();
  await clickAway(page);
  await page.keyboard.press('Control+z');
  await expect(readingList).toHaveCount(0);
});

test("starting up leaves nothing to undo: today's page stays", async ({
  page,
}) => {
  await page.goto('/');
  const first = page.locator('main section h2').first();
  await expect(first).toBeVisible();
  const today = await first.textContent();
  await clickAway(page);
  await page.keyboard.press('Control+z');
  await expect(first).toHaveText(today!);
});
