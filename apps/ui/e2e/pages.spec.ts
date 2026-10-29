import { expect, test, openEntity } from "./fixtures";

test("the journals feed starts at today, adds days as you scroll, and ends", async ({ page }) => {
  await page.goto("/");
  const days = page.locator("main section h2");
  await expect(days).toHaveCount(5);
  const main = page.locator("main");
  await expect(async () => {
    await main.evaluate((m) => m.scrollTo(0, m.scrollHeight));
    await expect(page.getByText("That's the lot.")).toBeVisible({ timeout: 250 });
  }).toPass();
  await expect(days).toHaveCount(30);
});

test("linked references are grouped by entity, newest journals first", async ({ page }) => {
  await openEntity(page, "Quiddity");
  const refs = page.getByRole("region", { name: "Linked references" });
  const titles = await refs.getByRole("heading", { level: 3 }).allTextContents();
  expect(titles.length).toBeGreaterThan(1);
  expect(titles).toEqual([...titles].sort().reverse());
});

test("unlinked references load on request and show mentions that aren't links", async ({ page }) => {
  await openEntity(page, "Loro");
  await page.getByText("Unlinked references").click();
  await expect(page.getByRole("heading", { name: "Reading list", level: 3 })).toBeVisible();
  await expect(page.getByText("Notes on loro and other CRDTs")).toBeVisible();
});

test("a link that goes somewhere works", async ({ page }) => {
  await openEntity(page, "Quiddity");
  const main = page.locator("main article > ul");
  await main.getByRole("link", { name: "Loro" }).click();
  await expect(page.getByRole("heading", { name: "Loro", level: 1 })).toBeVisible();
});

test("shift-click opens at the side, keeping the main page, and the card closes", async ({ page }) => {
  await openEntity(page, "Quiddity");
  await page.locator("main").getByRole("link", { name: "iroh", exact: true }).first().click({ modifiers: ["Shift"] });
  const card = page.locator("aside article");
  await expect(card.getByRole("heading", { name: "iroh" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Quiddity", level: 1 })).toBeVisible();
  await card.getByRole("link", { name: "Close" }).click();
  await expect(card).toHaveCount(0);
});

test("an unknown entity is reported in its own place, not the whole page", async ({ page }) => {
  await page.goto("/entity/?id=nonsense&open=also-nonsense");
  await expect(page.getByRole("alert").first()).toBeVisible();
  await expect(page.getByRole("navigation")).toBeVisible();
});

test("recents forget what no longer exists", async ({ page }) => {
  await openEntity(page, "Reading list");
  const recents = page.locator("nav section", { hasText: "Recents" });
  await expect(recents.getByRole("link", { name: "Reading list" })).toBeVisible();
  // A reload makes a new graph, so the remembered id now points at nothing.
  await page.goto("/");
  await expect(recents.getByText("Nothing yet.")).toBeVisible();
  await expect(recents.getByRole("link")).toHaveCount(0);
});

test("all entities is a table of words and incoming links", async ({ page }) => {
  await page.goto("/all/");
  const table = page.locator("main table");
  await expect(table.getByRole("columnheader")).toHaveText(["Title", "Tags", "Words", "Incoming links"]);
  const loro = table.getByRole("row").filter({ has: page.getByRole("link", { name: "Loro", exact: true }) });
  // "A CRDT library. Documents are movable trees of text." is linked from Quiddity.
  await expect(loro.getByRole("cell")).toHaveText(["", "9", "1"]);
});
