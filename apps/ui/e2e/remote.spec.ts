import { expect, test } from "./fixtures";

/** What the memory backend lets a test do as another device. */
declare global {
  interface Window {
    __quiddityRemote: { createEntity(name: string): void };
  }
}

test("changes from another device appear without reloading", async ({ page }) => {
  await page.goto("/all/");
  const main = page.locator("main");
  await expect(main.getByRole("link", { name: "Quiddity", exact: true }).first()).toBeVisible();
  const arrived = main.getByRole("link", { name: "From another device", exact: true });
  await expect(arrived).toHaveCount(0);

  await page.evaluate(() => window.__quiddityRemote.createEntity("From another device"));
  await expect(arrived.first()).toBeVisible();
});
