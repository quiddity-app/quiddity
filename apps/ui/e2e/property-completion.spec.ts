import { expect, test, editor, openEntity } from "./fixtures";

// CRDT's properties block is `kind of:: data structure` and `merges:: without
// coordination`, so those are the keys and values the graph already uses.
async function editProperties(page: import("@playwright/test").Page) {
  await openEntity(page, "CRDT");
  await page.locator("main article [data-properties-block] [data-block-text]").click();
  await page.keyboard.press("End");
}

test("a key being typed in a properties block is completed from the keys in use", async ({ page }) => {
  await editProperties(page);
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("mer");
  const list = page.getByRole("listbox");
  await expect(list.getByRole("option", { name: /^merges/ })).toBeVisible();
  // Enter chooses it, rather than splitting the block.
  await page.keyboard.press("Enter");
  await expect(list).toHaveCount(0);
  await expect(editor(page)).toContainText("merges:: ");
  await expect(page.locator("main article > ul > li")).toHaveCount(1);
});

test("a value is completed from the values the key already has", async ({ page }) => {
  await editProperties(page);
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("merges:: ");
  const option = page.getByRole("listbox").getByRole("option", { name: /^without coordination/ });
  await expect(option).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(editor(page)).toContainText("merges:: without coordination");
});

test("nothing is offered in an ordinary block", async ({ page }) => {
  await openEntity(page, "Quiddity");
  await page.locator("main article > ul").getByText("Local-first").click();
  await page.keyboard.press("End");
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("mer");
  await expect(page.getByRole("listbox")).toHaveCount(0);
});

test("the offered block is completed too, and Escape closes the list without leaving the block", async ({ page }) => {
  await openEntity(page, "iroh");
  await page.getByRole("button", { name: "+ Add properties" }).click();
  await page.keyboard.type("kin");
  await expect(page.getByRole("listbox").getByRole("option", { name: /^kind of/ })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("listbox")).toHaveCount(0);
  await expect(editor(page)).toBeFocused();
});
