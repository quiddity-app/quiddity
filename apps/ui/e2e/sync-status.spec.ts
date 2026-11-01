import { expect, test } from "./fixtures";

const states = [
  ["alone", "No other devices to sync with"],
  ["offline", "Not connected to any other device"],
  ["behind", "Connected, but out of date"],
  ["synced", "Connected and up to date"],
  ["present", "Another device is here"],
] as const;

test("the sync indicator sits left of search, and starts with nobody to sync with", async ({ page }) => {
  await page.goto("/");
  const indicator = page.getByRole("link", { name: /^Sync:/ });
  await expect(indicator).toHaveAttribute("data-state", "alone");
  await expect(indicator).toHaveAccessibleName("Sync: No other devices to sync with");

  const here = await indicator.boundingBox();
  const search = await page.getByRole("button", { name: /^Search/ }).boundingBox();
  expect(here!.x + here!.width).toBeLessThanOrEqual(search!.x);
});

test("it shows each state the shell can report", async ({ page }) => {
  await page.goto("/");
  const indicator = page.getByRole("link", { name: /^Sync:/ });
  await expect(indicator).toBeVisible();
  for (const [state, label] of states) {
    await page.evaluate((state) => window.__quiddityRemote!.setStatus(state), state);
    await expect(indicator).toHaveAttribute("data-state", state);
    await expect(indicator).toHaveAccessibleName(`Sync: ${label}`);
  }
});

test("each state has a colour of its own", async ({ page }) => {
  await page.goto("/");
  const indicator = page.getByRole("link", { name: /^Sync:/ });
  await expect(indicator).toBeVisible();
  const colours: string[] = [];
  for (const [state] of states) {
    await page.evaluate((state) => window.__quiddityRemote!.setStatus(state), state);
    await expect(indicator).toHaveAttribute("data-state", state);
    colours.push(
      await indicator.locator("span").evaluate((dot) => getComputedStyle(dot).backgroundColor),
    );
  }
  expect(new Set(colours).size).toBe(states.length);
});

test("it leads to the Devices page", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("link", { name: /^Sync:/ }).click();
  await expect(page.getByRole("heading", { name: "Devices", level: 1 })).toBeVisible();
});
