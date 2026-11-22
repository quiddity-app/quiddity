import { expect, test } from "./fixtures";

test("a system with no contacts has no section for them", async ({ page }) => {
  await page.goto("/devices/");
  await expect(page.getByRole("heading", { name: "This device" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Contacts" })).toHaveCount(0);
});

test("contacts are turned on and off, asking the shell each time", async ({ page }) => {
  await page.goto("/devices/?contacts");
  await expect(page.getByRole("heading", { name: "Contacts" })).toBeVisible();
  const state = page.getByRole("region", { name: "Contacts" }).getByRole("status");
  await expect(state).toHaveText("Off");

  await page.getByRole("button", { name: "Turn on" }).click();
  await expect(state).toHaveText("On");
  await page.getByRole("button", { name: "Turn off" }).click();
  await expect(state).toHaveText("Off");
  expect(await page.evaluate(() => window.__quiddityRemote?.contacts)).toEqual(["enable", "disable"]);
});

test("a permission that is refused says so and leaves them off", async ({ page }) => {
  await page.goto("/devices/?contacts=denied");
  await page.getByRole("button", { name: "Turn on" }).click();
  await expect(page.getByRole("region", { name: "Contacts" }).getByRole("alert")).toContainText("wasn't allowed");
  await expect(page.getByRole("region", { name: "Contacts" }).getByRole("status")).toHaveText("Off");
  await expect(page.getByRole("button", { name: "Turn on" })).toBeEnabled();
});
