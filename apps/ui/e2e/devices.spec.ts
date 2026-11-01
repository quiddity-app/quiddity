import { expect, test } from "./fixtures";

const mine = "f00d".repeat(16);
const theirs = "ab12".repeat(16);

test("the Devices page is in the sidebar and shows this device's ID", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("link", { name: "Devices", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Devices", level: 1 })).toBeVisible();
  await expect(page.getByRole("heading", { name: "This device" })).toBeVisible();
  await expect(page.getByText(mine)).toBeVisible();
});

test("adding a device asks the shell to let it in, and says so", async ({ page }) => {
  await page.goto("/devices/");
  const add = page.getByRole("button", { name: "Add device" });
  await expect(add).toBeDisabled();

  await page.getByLabel("Device ID").fill(` ${theirs} `);
  await add.click();
  await expect(page.getByRole("status")).toContainText("Added");
  await expect(page.getByLabel("Device ID")).toHaveValue("");
  expect(await page.evaluate(() => window.__quiddityRemote?.allowed)).toEqual([theirs]);
});

test("something that is not a device ID is refused, with the reason", async ({ page }) => {
  await page.goto("/devices/");
  await page.getByLabel("Device ID").fill("not a device");
  await page.getByRole("button", { name: "Add device" }).click();
  await expect(page.locator("main").getByRole("alert")).toContainText("is not a device ID");
  expect(await page.evaluate(() => window.__quiddityRemote?.allowed ?? [])).toEqual([]);
  // What was typed is kept to correct.
  await expect(page.getByLabel("Device ID")).toHaveValue("not a device");
});
