import { expect, test, editor, openEntity } from './fixtures';

import type { Page } from '@playwright/test';

// The Quiddity page:
//   properties
//   Local-first, multiplayer second brain.
//     Built on [[Loro]] and [[iroh]].
//     Shift-click a link to open it at the side.
//   Open questions
//     Eviction of unsynced edits.
const PARENT = 'Local-first, multiplayer second brain.';
const LEAF = 'Shift-click a link to open it at the side.';
const QUESTIONS = 'Open questions';

test.beforeEach(async ({ page }) => {
  await openEntity(page, 'Quiddity');
});

const block = (page: Page, text: string) =>
  page.locator('main article > ul').getByText(text, { exact: true });

/** What a screen reader is told, as a polite live region. */
const said = (page: Page) =>
  page.locator('main [role=status][aria-live=polite]').first();

/** Presses a key, and checks what a screen reader is then told. */
async function hears(page: Page, key: string, text: string) {
  await page.keyboard.press(key);
  await expect(said(page)).toHaveText(text);
}

/** Selects the block with this text, and as many levels more as `more` says. */
async function select(page: Page, text: string, more = 0) {
  await block(page, text).click();
  for (let i = 0; i < 2 + more; i++) await page.keyboard.press('Control+a');
}

test('selecting says how many blocks, and reads out a single one', async ({
  page,
}) => {
  await select(page, LEAF);
  await expect(said(page)).toHaveText(`1 block selected: ${LEAF}`);
  await hears(page, 'Control+a', '2 blocks selected');
  await hears(page, 'Control+a', '3 blocks selected');
  await hears(page, 'Escape', 'Selection cleared');
});

test("what an edit did is said, including when it couldn't", async ({
  page,
}) => {
  // Open questions, and what is under it.
  await select(page, QUESTIONS, 1);
  await hears(page, 'Tab', 'Indented');
  await hears(page, 'Shift+Tab', 'Outdented');

  await hears(page, 'Alt+Shift+ArrowUp', 'Moved up');
  await hears(page, 'Alt+Shift+ArrowDown', 'Moved down');
  await hears(page, 'Alt+Shift+ArrowDown', "Can't move down");
});

test("the first of a run of siblings can't be indented, and it says so", async ({
  page,
}) => {
  // The parent's two children: the first has nothing before it.
  await select(page, LEAF, 1);
  await hears(page, 'Tab', "Can't indent");
});

test('the same thing said twice is said twice', async ({ page }) => {
  await select(page, QUESTIONS, 1);
  await hears(page, 'Alt+Shift+ArrowUp', 'Moved up');
  const first = await said(page).evaluate((el) => el.textContent);
  await page.keyboard.press('Alt+Shift+ArrowUp');
  await expect
    .poll(() => said(page).evaluate((el) => el.textContent))
    .not.toBe(first);
  await expect(said(page)).toHaveText('Moved up');
});

test('delete, copy, cut and paste say how many blocks', async ({ page }) => {
  await select(page, QUESTIONS, 1);
  await hears(page, 'Control+c', '2 blocks copied');
  await hears(page, 'Control+v', '2 blocks pasted');
  // What was pasted is selected, so it can be deleted.
  await hears(page, 'Backspace', '2 blocks deleted');

  await select(page, QUESTIONS, 1);
  await hears(page, 'Control+x', '2 blocks cut');
});

test("the editor's own keys say what they did too", async ({ page }) => {
  await block(page, LEAF).click();
  await expect(editor(page)).toBeFocused();
  await hears(page, 'Tab', 'Indented');
  await hears(page, 'Shift+Tab', 'Outdented');
  await hears(page, 'Alt+Shift+ArrowUp', 'Moved up');
});
