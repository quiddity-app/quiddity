import { expect, test, mainBlocks, openEntity } from "./fixtures";

const block = (page: import("@playwright/test").Page) =>
  page.locator("main article > ul").getByText("Local-first");

test.beforeEach(async ({ page }) => {
  await openEntity(page, "Quiddity");
});

async function append(page: import("@playwright/test").Page, text: string) {
  await block(page).click();
  await page.keyboard.press("End");
  await page.keyboard.type(text);
}

test("saving a block makes an entity for a link it gained, which can be followed", async ({ page }) => {
  await append(page, " and [[Fresh]]");
  await page.keyboard.press("Escape");
  const link = page.locator("main article > ul").getByRole("link", { name: "Fresh" });
  await expect(link).toBeVisible();
  await link.click();
  await expect(page.getByRole("heading", { name: "Fresh", level: 1 })).toBeVisible();
  // It has an empty block to type into, and Quiddity links to it.
  expect(await mainBlocks(page)).toEqual([""]);
  await expect(page.getByRole("heading", { name: /Linked references/ })).toBeVisible();
});

test("correcting the link before using the entity renames it, with no alias", async ({ page }) => {
  await append(page, " and [[Frsh]]");
  await page.keyboard.press("Escape");
  await expect(page.locator("main article > ul").getByRole("link", { name: "Frsh" })).toBeVisible();

  // Back into the block, and fix the typo from inside the brackets.
  await block(page).click();
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Backspace");
  await page.keyboard.type("resh");
  await page.keyboard.press("Escape");

  const outline = page.locator("main article > ul");
  await expect(outline.getByRole("link", { name: "Fresh" })).toBeVisible();
  await outline.getByRole("link", { name: "Fresh" }).click();
  await expect(page.getByRole("heading", { name: "Fresh", level: 1 })).toBeVisible();
  expect(await mainBlocks(page), "no alias block").toEqual([""]);

  await page.locator("nav").getByRole("link", { name: "All entities" }).click();
  const all = page.locator("main");
  await expect(all.getByRole("link", { name: "Fresh", exact: true })).toBeVisible();
  await expect(all.getByRole("link", { name: "Frsh", exact: true })).toHaveCount(0);
});

test("a link left dangling makes its entity when followed", async ({ page }) => {
  await append(page, " and [[Gone]]");
  await page.keyboard.press("Escape");
  // Undo takes the entity away again, leaving the link to nowhere.
  await page.keyboard.press("Control+z");
  const outline = page.locator("main article > ul");
  // It is a link like any other, to assistive technology and to the eye.
  const dangling = outline.getByRole("link", { name: "Gone" });
  await expect(dangling).toHaveAttribute("href", "#");
  await expect(dangling).toHaveClass(/text-link/);
  await expect(dangling).not.toHaveClass(/dashed/);
  await dangling.click();
  await expect(page.getByRole("heading", { name: "Gone", level: 1 })).toBeVisible();
});
