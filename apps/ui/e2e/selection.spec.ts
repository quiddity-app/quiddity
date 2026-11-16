import type { Page } from "@playwright/test";
import { expect, test, editor, openEntity } from "./fixtures";

// The Quiddity page, which has the nesting:
//   properties
//   Local-first, multiplayer second brain.
//     Built on [[Loro]] and [[iroh]].
//     Shift-click a link to open it at the side.
//   Open questions
//     Eviction of unsynced edits.
const PARENT = "Local-first, multiplayer second brain.";
const LEAF = "Shift-click a link to open it at the side.";
const QUESTIONS = "Open questions";
const EVICTION = "Eviction of unsynced edits.";

test.beforeEach(async ({ page }) => {
  await openEntity(page, "Quiddity");
});

const block = (page: Page, text: string) =>
  page.locator("main article > ul").getByText(text, { exact: true });

/** The text of each block selected as a block, in reading order. */
const selected = (page: Page) =>
  page.locator("main article [data-selected]").evaluateAll((els) => els.map((e) => e.textContent));

test("Select All grows from the block through its family to the whole entity", async ({ page }) => {
  await block(page, LEAF).click();
  await expect(editor(page)).toBeFocused();

  // The first press is the editor's own: the block's text.
  await page.keyboard.press("Control+a");
  await expect(editor(page)).toBeFocused();
  expect(await selected(page)).toEqual([]);

  // Then the block, as a block: it stops being edited.
  await page.keyboard.press("Control+a");
  await expect(editor(page)).toHaveCount(0);
  expect(await selected(page)).toEqual([LEAF]);

  // It has no children, so next is it and its siblings.
  await page.keyboard.press("Control+a");
  expect(await selected(page)).toHaveLength(2);

  // Then the parent, which brings its children.
  await page.keyboard.press("Control+a");
  expect(await selected(page)).toHaveLength(3);
  expect(await selected(page)).toContain(PARENT);

  // Then the parent's siblings and what is under them: everything.
  await page.keyboard.press("Control+a");
  expect(await selected(page)).toHaveLength(6);
  expect(await selected(page)).toContain(EVICTION);

  // There is nothing more.
  await page.keyboard.press("Control+a");
  expect(await selected(page)).toHaveLength(6);
});

test("a block with children selects them next", async ({ page }) => {
  await block(page, PARENT).click();
  await page.keyboard.press("Control+a");
  await page.keyboard.press("Control+a");
  expect(await selected(page)).toEqual([PARENT]);
  await page.keyboard.press("Control+a");
  expect(await selected(page)).toHaveLength(3);
});

test("Escape, or a press elsewhere, lets go", async ({ page }) => {
  await block(page, LEAF).click();
  await page.keyboard.press("Control+a");
  await page.keyboard.press("Control+a");
  expect(await selected(page)).toHaveLength(1);
  await page.keyboard.press("Escape");
  expect(await selected(page)).toEqual([]);

  await block(page, LEAF).click();
  await page.keyboard.press("Control+a");
  await page.keyboard.press("Control+a");
  await page.locator("body").click({ position: { x: 5, y: 5 } });
  expect(await selected(page)).toEqual([]);
});

test("Select All in the title is left to the title", async ({ page }) => {
  await page.getByRole("heading", { name: "Quiddity", level: 1 }).click();
  await page.keyboard.press("Control+a");
  expect(await selected(page)).toEqual([]);
});

test("dragging from one block to another selects the blocks between", async ({ page }) => {
  const from = (await block(page, PARENT).boundingBox())!;
  const to = (await block(page, EVICTION).boundingBox())!;
  await page.mouse.move(from.x + 10, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(to.x + 10, to.y + to.height / 2, { steps: 8 });
  await page.mouse.up();

  expect(await selected(page)).toEqual([
    PARENT,
    "Built on Loro and iroh.",
    LEAF,
    QUESTIONS,
    EVICTION,
  ]);
  // Nothing was opened for editing by the release.
  await expect(editor(page)).toHaveCount(0);

  // Select All carries on from there: here, that is everything.
  await page.keyboard.press("Control+a");
  expect(await selected(page)).toHaveLength(6);
});

test("a drag that starts in the block being edited leaves it for a block selection", async ({
  page,
}) => {
  await block(page, PARENT).click();
  await expect(editor(page)).toBeFocused();
  const to = (await block(page, QUESTIONS).boundingBox())!;
  const from = (await editor(page).boundingBox())!;
  await page.mouse.move(from.x + 10, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(to.x + 10, to.y + to.height / 2, { steps: 8 });
  await page.mouse.up();

  await expect(editor(page)).toHaveCount(0);
  expect((await selected(page)).at(0)).toBe(PARENT);
  expect((await selected(page)).at(-1)).toBe(QUESTIONS);
});

test("dragging within one block selects text, not blocks, and a click still edits", async ({
  page,
}) => {
  const box = (await block(page, LEAF).boundingBox())!;
  await page.mouse.move(box.x + 5, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + 60, box.y + box.height / 2, { steps: 4 });
  await page.mouse.up();
  expect(await selected(page)).toEqual([]);

  await page.locator("body").click({ position: { x: 5, y: 5 } });
  await block(page, LEAF).click();
  await expect(editor(page)).toBeFocused();
});

/** How many blocks are at the top level of the main pane. */
const topLevel = (page: Page) => page.locator("main article > ul > li").count();

test("Tab and Shift-Tab indent and outdent what is selected, in one undo step", async ({ page }) => {
  expect(await topLevel(page)).toBe(3);
  // Open questions, with what is under it.
  await block(page, QUESTIONS).click();
  for (let i = 0; i < 3; i++) await page.keyboard.press("Control+a");
  expect(await selected(page)).toEqual([QUESTIONS, EVICTION]);

  // It nests under the block before it, and what was under it goes along.
  await page.keyboard.press("Tab");
  await expect.poll(() => topLevel(page)).toBe(2);
  expect(await selected(page)).toEqual([QUESTIONS, EVICTION]);

  await page.keyboard.press("Shift+Tab");
  await expect.poll(() => topLevel(page)).toBe(3);

  await page.keyboard.press("Tab");
  await expect.poll(() => topLevel(page)).toBe(2);
  await page.keyboard.press("Control+z");
  await expect.poll(() => topLevel(page)).toBe(3);
});

test("a run with nothing before it can't be indented", async ({ page }) => {
  // The parent's two children: the first has no sibling to nest under.
  await block(page, LEAF).click();
  for (let i = 0; i < 3; i++) await page.keyboard.press("Control+a");
  expect(await selected(page)).toHaveLength(2);
  await page.keyboard.press("Tab");
  expect(await topLevel(page)).toBe(3);
  expect(await selected(page)).toHaveLength(2);
});

test("Tab with nothing selected is left to the browser", async ({ page }) => {
  await page.keyboard.press("Tab");
  expect(await selected(page)).toEqual([]);
  expect(await topLevel(page)).toBe(3);
});

/** The text of each top-level block of the main pane. */
const topTexts = (page: Page) =>
  page
    .locator("main article > ul > li > div > [data-block-text]")
    .evaluateAll((els) => els.map((e) => e.textContent));

// Alt-Shift is how a block is moved except on a Mac, which the tests aren't on.
test("Alt-Shift-arrows move the selected blocks, with their children, in one undo step", async ({
  page,
}) => {
  const before = await topTexts(page);
  expect(before.at(-1)).toBe(QUESTIONS);
  await block(page, QUESTIONS).click();
  for (let i = 0; i < 3; i++) await page.keyboard.press("Control+a");

  await page.keyboard.press("Alt+Shift+ArrowUp");
  await expect.poll(() => topTexts(page)).toEqual([before[0], QUESTIONS, PARENT]);
  expect(await selected(page)).toEqual([QUESTIONS, EVICTION]);

  await page.keyboard.press("Alt+Shift+ArrowDown");
  await expect.poll(() => topTexts(page)).toEqual(before);

  await page.keyboard.press("Alt+Shift+ArrowUp");
  await expect.poll(() => topTexts(page)).not.toEqual(before);
  await page.keyboard.press("Control+z");
  await expect.poll(() => topTexts(page)).toEqual(before);
});

test("the move keys move the focused block, though nothing is selected", async ({ page }) => {
  const before = await topTexts(page);
  // The block's own element, which is what a Tab to it focuses.
  const questions = page
    .locator("main article > ul > li > div > [data-block-text]")
    .filter({ hasText: QUESTIONS });
  await questions.focus();
  await page.keyboard.press("Alt+Shift+ArrowUp");
  await expect.poll(() => topTexts(page)).toEqual([before[0], QUESTIONS, PARENT]);
  expect(await selected(page)).toEqual([]);
  // It keeps the focus, so it can go on.
  await expect(questions).toBeFocused();
  await page.keyboard.press("Alt+Shift+ArrowUp");
  await expect.poll(() => topTexts(page)).toEqual([QUESTIONS, before[0], PARENT]);
});

test("a selection that includes the first block can't move up", async ({ page }) => {
  const before = await topTexts(page);
  await block(page, PARENT).click();
  // Everything, which has the first block in it.
  for (let i = 0; i < 6; i++) await page.keyboard.press("Control+a");
  expect(await selected(page)).toHaveLength(6);
  await page.keyboard.press("Alt+Shift+ArrowUp");
  expect(await topTexts(page)).toEqual(before);
  await page.keyboard.press("Alt+Shift+ArrowDown");
  expect(await topTexts(page)).toEqual(before);
});
