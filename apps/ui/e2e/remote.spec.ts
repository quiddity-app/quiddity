import { expect, test } from "./fixtures";

test("changes from another device appear without reloading", async ({ page }) => {
  await page.goto("/all/");
  const main = page.locator("main");
  await expect(main.getByRole("link", { name: "Quiddity", exact: true }).first()).toBeVisible();
  const arrived = main.getByRole("link", { name: "From another device", exact: true });
  await expect(arrived).toHaveCount(0);

  await page.evaluate(() => window.__quiddityRemote!.createEntity("From another device"));
  await expect(arrived.first()).toBeVisible();
});

test("a page that has joined no graph makes nothing until one arrives", async ({ page }) => {
  await page.goto("/all/?blank");
  await expect(page.getByRole("button", { name: /search/i })).toBeVisible();
  const main = page.locator("main");
  await page.waitForFunction(() => window.__quiddityRemote);
  // Long enough for an edit made on load to have been pushed.
  await page.waitForTimeout(1500);
  expect(await page.evaluate(() => window.__quiddityRemote!.typed.length)).toBe(0);

  await page.evaluate(() => window.__quiddityRemote!.createEntity("From another device"));
  await expect(main.getByRole("link", { name: "From another device", exact: true }).first()).toBeVisible();
});
