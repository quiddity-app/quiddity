import { expect, test, openEntity } from "./fixtures";

const heading = (page: import("@playwright/test").Page, name: string) =>
  page.getByRole("heading", { name, level: 1 });

for (const { label, back, forward } of [
  { label: "Alt with the arrows", back: "Alt+ArrowLeft", forward: "Alt+ArrowRight" },
  { label: "the square brackets", back: "Control+[", forward: "Control+]" },
]) {
  test(`${label} step the main pane and leave the side panel alone`, async ({ page }) => {
    await openEntity(page, "Quiddity");
    // Go somewhere else in the main pane, then open a card at the side.
    await page.locator("main article > ul").getByRole("link", { name: "Loro" }).first().click();
    await expect(heading(page, "Loro")).toBeVisible();
    await page.locator("nav").getByRole("link", { name: "Journals" }).click();
    await expect(page).toHaveURL(/\/journals\/$/);
    await page.locator("main").getByRole("link").first().click({ modifiers: ["Shift"] });
    await expect(page).toHaveURL(/open=/);
    const open = new URL(page.url()).searchParams.get("open");

    await page.keyboard.press(back);
    await expect(heading(page, "Loro")).toBeVisible();
    expect(new URL(page.url()).searchParams.get("open")).toBe(open);

    await page.keyboard.press(back);
    await expect(heading(page, "Quiddity")).toBeVisible();
    expect(new URL(page.url()).searchParams.get("open")).toBe(open);

    await page.keyboard.press(forward);
    await expect(heading(page, "Loro")).toBeVisible();
    expect(new URL(page.url()).searchParams.get("open")).toBe(open);
  });
}

test("opening and closing a card isn't a step, so forward still works after one", async ({ page }) => {
  const links = page.locator("main article > ul");
  await openEntity(page, "Quiddity");
  await links.getByRole("link", { name: "Loro" }).first().click();
  await expect(heading(page, "Loro")).toBeVisible();
  await page.keyboard.press("Alt+ArrowLeft");
  await expect(heading(page, "Quiddity")).toBeVisible();

  await links.getByRole("link", { name: "Loro" }).first().click({ modifiers: ["Shift"] });
  await expect(page).toHaveURL(/open=/);
  await page.getByRole("link", { name: "Close all" }).click();
  await expect(page).not.toHaveURL(/open=/);

  await page.keyboard.press("Alt+ArrowRight");
  await expect(heading(page, "Loro")).toBeVisible();
});
