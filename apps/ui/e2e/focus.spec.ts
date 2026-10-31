import { clickAway, editBlock, expect, openEntity, test } from "./fixtures";

const block = "A CRDT library. Documents are movable trees of text.";

test("the block being edited is reported, and stops being when editing does", async ({ page }) => {
  await openEntity(page, "Loro");
  await editBlock(page, block);
  const focus = () => page.evaluate(() => window.__quiddityRemote?.focus.length);
  await expect.poll(focus).toBe(1);
  await clickAway(page);
  await expect.poll(focus).toBe(0);
});

test("moving to another block is told as one move", async ({ page }) => {
  await openEntity(page, "Quiddity");
  await editBlock(page, "Local-first, multiplayer second brain.");
  const focus = () => page.evaluate(() => window.__quiddityRemote?.focus.join(",") ?? "");
  await expect.poll(focus).toMatch(/^[^,]+$/);
  const first = await focus();
  await clickAway(page);
  await editBlock(page, "Open questions");
  await expect.poll(focus).toMatch(/^[^,]+$/);
  expect(await focus()).not.toBe(first);
});

test("typing is pushed as typing, and anything else as itself", async ({ page }) => {
  await openEntity(page, "Loro");
  await editBlock(page, block);
  await page.keyboard.press("End");
  await page.keyboard.type(" Typed.");
  const pushes = () => page.evaluate(() => window.__quiddityRemote?.typed ?? []);
  await expect.poll(async () => (await pushes()).at(-1)?.length).toBe(1);

  // Splitting the block is structure, so it isn't held back.
  await page.keyboard.press("Enter");
  await expect.poll(async () => (await pushes()).at(-1)?.length).toBe(0);
  expect((await pushes()).length).toBeGreaterThan(1);
});
