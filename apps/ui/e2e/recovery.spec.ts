import { expect, test } from "./fixtures";

test("a graph that opened shows no recovery screen", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("link", { name: "Devices", exact: true })).toBeVisible();
  await expect(page.getByRole("alertdialog")).toHaveCount(0);
});

test("a graph that couldn't be opened says why, where, and that nothing is deleted", async ({ page }) => {
  await page.goto("/?damaged");
  const dialog = page.getByRole("alertdialog", { name: /can’t open this device’s graph/ });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("/home/someone/graph");
  await expect(dialog).toContainText("unexpected end of data");
  await expect(dialog).toContainText("Nothing has been deleted");
  await expect(dialog.getByRole("button", { name: "Try again" })).toBeFocused();
});

for (const button of ["Try again", "Start empty and sync", "Start a new graph"]) {
  test(`${button} goes on, and the page starts over without the problem`, async ({ page }) => {
    await page.goto("/?damaged");
    await page.getByRole("button", { name: button }).click();
    // The shell has cleared the problem, so the reload has none to show.
    await expect(page.getByRole("alertdialog")).toHaveCount(0);
    expect(page.url()).not.toContain("damaged");
  });
}

test("each choice is what the shell is asked for", async ({ page }) => {
  await page.goto("/?damaged=stubborn");
  for (const [button, how] of [
    ["Try again", "retry"],
    ["Start empty and sync", "blank"],
    ["Start a new graph", "fresh"],
  ]) {
    await page.getByRole("button", { name: button }).click();
    await expect(page.getByRole("alertdialog").getByRole("alert")).toContainText("permission denied");
    const asked = await page.evaluate(() => window.__quiddityRemote?.recovered);
    expect(asked?.at(-1)).toBe(how);
  }
});

test("when going on fails, it says why and the choices are still there", async ({ page }) => {
  await page.goto("/?damaged=stubborn");
  await page.getByRole("button", { name: "Start empty and sync" }).click();
  await expect(page.getByRole("alertdialog").getByRole("alert")).toContainText("permission denied");
  await expect(page.getByRole("button", { name: "Try again" })).toBeEnabled();
  await expect(page.getByRole("alertdialog")).toBeVisible();
});
