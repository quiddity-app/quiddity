import { expect, test, editBlock, editor, openEntity } from "./fixtures";

/** Where the caret is, against the edges of the pane that scrolls. */
const caret = (page: import("@playwright/test").Page) =>
  page.evaluate(() => {
    const main = document.querySelector("main")!.getBoundingClientRect();
    const range = getSelection()!.getRangeAt(0);
    const box = range.getClientRects()[0] ?? range.startContainer.parentElement!.getBoundingClientRect();
    return { top: Math.round(box.top - main.top), bottom: Math.round(main.bottom - box.bottom) };
  });

const scrollTop = (page: import("@playwright/test").Page) =>
  page.evaluate(() => document.querySelector("main")!.scrollTop);

test("the caret stays in view as Enter adds blocks past the bottom, without the page jumping back", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1200, height: 600 });
  await openEntity(page, "Reading list");
  await editBlock(page, "Designing Data-Intensive Applications");
  await page.keyboard.press("End");
  let last = await scrollTop(page);
  for (let i = 0; i < 25; i++) {
    await page.keyboard.press("Enter");
    await expect(editor(page)).toHaveText("");
    // Before anything is typed into the new block. The scroll happens in
    // CodeMirror's next measure, a frame on, so wait for it rather than sample.
    await expect
      .poll(async () => {
        const { top, bottom } = await caret(page);
        return top >= 0 && bottom >= 0;
      }, { message: `caret off the pane after Enter ${i}`, timeout: 1000 })
      .toBe(true);
    const now = await scrollTop(page);
    expect(now, `scrolled back after Enter ${i}`).toBeGreaterThanOrEqual(last);
    last = now;
  }
});

test("Enter in the middle of a long page leaves the view where it was", async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 600 });
  await openEntity(page, "Reading list");
  await editBlock(page, "Designing Data-Intensive Applications");
  await page.keyboard.press("End");
  for (let i = 0; i < 25; i++) {
    await page.keyboard.press("Enter");
    await page.keyboard.type(`line ${i}`);
  }
  await page.keyboard.press("Escape");
  await page.evaluate(() => (document.querySelector("main")!.scrollTop = 400));
  await page.getByText("line 8", { exact: true }).click();
  await page.keyboard.press("End");
  const before = await scrollTop(page);
  await page.keyboard.press("Enter");
  await expect(editor(page)).toHaveText("");
  expect(await scrollTop(page)).toBe(before);
});
