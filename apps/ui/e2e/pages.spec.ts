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
  await expect(table.getByRole("columnheader")).toContainText(["", "Title", "Tags", "Words", "Incoming links", "Created", "Last modified"]);
  const loro = table.getByRole("row").filter({ has: page.getByRole("link", { name: "Loro", exact: true }) });
  // "A CRDT library. Documents are movable trees of text." is linked from Quiddity.
  await expect(loro.getByRole("cell")).toHaveText(["", "", "9", "1", /./, /./]);
});

test("the all entities table sorts by a column, and the other way on a second click", async ({ page }) => {
  await page.goto("/all/");
  const table = page.locator("main table");
  const words = async () =>
    (await table.locator("tbody tr td:nth-child(4)").allTextContents()).map(Number);
  await table.getByRole("button", { name: "Words" }).click();
  await expect(table.getByRole("columnheader", { name: /Words/ })).toHaveAttribute("aria-sort", "ascending");
  const up = await words();
  expect(up).toEqual([...up].sort((a, b) => a - b));
  await table.getByRole("button", { name: "Words" }).click();
  await expect(table.getByRole("columnheader", { name: /Words/ })).toHaveAttribute("aria-sort", "descending");
  const down = await words();
  expect(down).toEqual([...down].sort((a, b) => b - a));
});

test("the all entities table shows when each entity was made and last changed", async ({ page }) => {
  await page.goto("/all/");
  const loro = page.locator("main table").getByRole("row").filter({ has: page.getByRole("link", { name: "Loro", exact: true }) });
  await expect(loro.locator("time")).toHaveCount(2);
  const times = await loro.locator("time").evaluateAll((ts) => ts.map((t) => Date.parse(t.getAttribute("datetime") ?? "")));
  const [created, modified] = times as [number, number];
  expect(Math.abs(Date.now() - created)).toBeLessThan(10 * 60_000);
  expect(modified).toBeGreaterThanOrEqual(created);
});

test("selected entities are deleted from the all entities table once confirmed", async ({ page }) => {
  await page.goto("/all/");
  const table = page.locator("main table");
  const rows = table.locator("tbody tr");
  // The graph loads in the browser, so there are no rows to count at first.
  await expect(rows.first()).toBeVisible();
  const before = await rows.count();
  const row = (name: string) => rows.filter({ has: page.getByRole("link", { name, exact: true }) });
  await expect(page.getByRole("button", { name: /Delete/ })).toHaveCount(0);

  await table.getByRole("checkbox", { name: "Select Loro", exact: true }).check();
  await table.getByRole("checkbox", { name: "Select Quiddity", exact: true }).check();
  await expect(page.getByText("2 selected")).toBeVisible();

  // Asking changes nothing, and neither does changing one's mind.
  await page.getByRole("button", { name: "Delete…" }).click();
  await expect(page.getByRole("alertdialog")).toContainText("Delete 2 entities");
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(rows).toHaveCount(before);
  await expect(page.getByRole("alertdialog")).toHaveCount(0);

  // Choosing again withdraws the question.
  await page.getByRole("button", { name: "Delete…" }).click();
  await table.getByRole("checkbox", { name: "Select Quiddity", exact: true }).uncheck();
  await expect(page.getByRole("alertdialog")).toHaveCount(0);
  await page.getByRole("button", { name: "Delete…" }).click();
  await expect(page.getByRole("alertdialog")).toContainText("Delete “Loro”");

  await page.getByRole("alertdialog").getByRole("button", { name: "Delete" }).click();
  await expect(rows).toHaveCount(before - 1);
  await expect(row("Loro")).toHaveCount(0);
  await expect(row("Quiddity")).toHaveCount(1);
  await expect(page.getByRole("toolbar", { name: "Selected entities" })).toHaveText("");
  await expect(table.getByRole("checkbox", { name: "Select Quiddity", exact: true })).not.toBeChecked();

  // The deletion is the user's, so it can be undone.
  await page.keyboard.press("Control+Z");
  await expect(row("Loro")).toHaveCount(1);
});

test("the all entities table selects every row at once", async ({ page }) => {
  await page.goto("/all/");
  const table = page.locator("main table");
  const everyone = table.getByRole("checkbox", { name: "Select all" });
  await expect(table.locator("tbody tr").first()).toBeVisible();
  const count = await table.locator("tbody tr").count();
  await table.getByRole("checkbox", { name: "Select Loro", exact: true }).check();
  expect(await everyone.evaluate((e: HTMLInputElement) => e.indeterminate)).toBe(true);
  await everyone.check();
  await expect(page.getByText(`${count} selected`)).toBeVisible();
  await everyone.uncheck();
  await expect(table.locator("tbody input:checked")).toHaveCount(0);
});
