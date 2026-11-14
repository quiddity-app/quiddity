import { expect, test, clickAway, openEntity } from "./fixtures";

const title = (page: import("@playwright/test").Page) =>
  page.locator("main article h1");

async function rename(page: import("@playwright/test").Page, to: string) {
  await title(page).getByRole("button").click();
  await page.getByLabel("Name").fill(to);
  await page.keyboard.press("Enter");
}

test("an entity is renamed in place, and the lists and sidebar follow", async ({ page }) => {
  await openEntity(page, "Loro");
  await rename(page, "Loro CRDT");
  await expect(page.getByRole("heading", { name: "Loro CRDT", level: 1 })).toBeVisible();
  // It was a favourite and a recent: both now say the new name.
  await expect(page.locator("nav").getByRole("link", { name: "Loro CRDT" }).first()).toBeVisible();
  await expect(page.locator("nav").getByRole("link", { name: "Loro", exact: true })).toHaveCount(0);
  // By the sidebar, since a reload would start over from the made-up graph.
  await page.locator("nav").getByRole("link", { name: "All entities" }).click();
  await expect(page.locator("main").getByRole("link", { name: "Loro CRDT" })).toBeVisible();
});

test("the old name stays, as an alias property, and links to it still work", async ({ page }) => {
  await openEntity(page, "Loro");
  await rename(page, "Loro CRDT");
  const block = page.locator("main article > ul [data-block-text]").first();
  await expect(block.getByText("alias:: Loro")).toBeVisible();

  // Quiddity still says [[Loro]], and that still leads here, not to a dead link.
  await page.locator("nav").getByRole("link", { name: "Quiddity" }).first().click();
  await page.locator("main article > ul").getByRole("link", { name: "Loro", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Loro CRDT", level: 1 })).toBeVisible();
});

test("Escape, an empty name and the same name change nothing", async ({ page }) => {
  await openEntity(page, "Loro");
  await title(page).getByRole("button").click();
  await page.getByLabel("Name").fill("Dropped");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("heading", { name: "Loro", level: 1 })).toBeVisible();

  await rename(page, "   ");
  await expect(page.getByRole("heading", { name: "Loro", level: 1 })).toBeVisible();
  await rename(page, "Loro");
  await expect(page.getByRole("heading", { name: "Loro", level: 1 })).toBeVisible();
  await expect(page.locator("main article > ul").getByText("alias::", { exact: true })).toHaveCount(0);
});

test("leaving the field keeps the new name", async ({ page }) => {
  await openEntity(page, "iroh");
  await title(page).getByRole("button").click();
  await page.getByLabel("Name").fill("iroh (QUIC)");
  await clickAway(page);
  await expect(page.getByRole("heading", { name: "iroh (QUIC)", level: 1 })).toBeVisible();
});

test("one undo takes back the name and the alias together", async ({ page }) => {
  await openEntity(page, "iroh");
  await rename(page, "Iroh");
  await rename(page, "Iroh networking");
  await clickAway(page);
  await page.keyboard.press("Control+z");
  await expect(page.getByRole("heading", { name: "Iroh", level: 1 })).toBeVisible();
  await page.keyboard.press("Control+z");
  await expect(page.getByRole("heading", { name: "iroh", level: 1 })).toBeVisible();
  await expect(page.locator("main article > ul").getByText("alias::", { exact: true })).toHaveCount(0);
});

test("a journal's date is its name: it is a heading, not a field", async ({ page }) => {
  await page.goto("/");
  await page.locator("main section h2 a").first().click();
  await expect(page.locator("main article h1")).toBeVisible();
  await expect(title(page).getByRole("button")).toHaveCount(0);
});
