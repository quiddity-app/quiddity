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

test("a link to a name nobody has made opens an empty entity, made by writing to it", async ({ page }) => {
  await append(page, " and [[Fresh]]");
  await page.keyboard.press("Escape");
  const link = page.locator("main article > ul").getByRole("link", { name: "Fresh" });
  await expect(link).toBeVisible();
  await link.click();
  await expect(page.getByRole("heading", { name: "Fresh", level: 1 })).toBeVisible();
  // It looks like an empty entity, and Quiddity links to it.
  expect(await mainBlocks(page)).toEqual([]);
  await expect(page.getByRole("heading", { name: /Linked references/ })).toBeVisible();
  // Writing to it makes it real.
  await page.getByText("Empty. Click to add a block.").click();
  await page.keyboard.type("Fresh notes");
  await page.keyboard.press("Escape");
  expect(await mainBlocks(page)).toEqual(["Fresh notes"]);
});

test("correcting a link leaves only the corrected name", async ({ page }) => {
  await append(page, " and [[Frsh]]");
  await page.keyboard.press("Escape");
  await block(page).click();
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Backspace");
  await page.keyboard.type("resh");
  await page.keyboard.press("Escape");

  await page.locator("nav").getByRole("link", { name: "All entities" }).click();
  const all = page.locator("main");
  await expect(all.getByRole("link", { name: "Fresh", exact: true })).toBeVisible();
  await expect(all.getByRole("link", { name: "Frsh", exact: true })).toHaveCount(0);
});

test("a link to nowhere is drawn like any other and opens its page", async ({ page }) => {
  await append(page, " and [[Gone]]");
  await page.keyboard.press("Escape");
  const outline = page.locator("main article > ul");
  // It is a link like any other, to assistive technology and to the eye.
  const dangling = outline.getByRole("link", { name: "Gone" });
  await expect(dangling).toHaveAttribute("href", /id=%7Egone/);
  await expect(dangling).toHaveClass(/text-link/);
  await expect(dangling).not.toHaveClass(/dashed/);
  await dangling.click();
  await expect(page.getByRole("heading", { name: "Gone", level: 1 })).toBeVisible();
});
