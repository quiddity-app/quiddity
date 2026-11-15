import { expect, openEntity, test } from "./fixtures";

test("the More menu holds Devices, which is no longer in the sidebar", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("link", { name: "Journals", exact: true })).toBeVisible();
  await expect(page.getByRole("link", { name: "Devices", exact: true })).toHaveCount(0);

  const more = page.getByRole("button", { name: "More" });
  await more.click();
  await expect(page.getByRole("menu", { name: "More" })).toBeVisible();
  // Escape closes it and gives the focus back.
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).toHaveCount(0);
  await expect(more).toBeFocused();

  await more.click();
  await page.getByRole("menuitem", { name: "Devices", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Devices", level: 1 })).toBeVisible();
  await expect(page.getByRole("menu")).toHaveCount(0);
});

test("the More menu sits to the right of search", async ({ page }) => {
  await page.goto("/");
  const search = page.getByRole("button", { name: /^Search/ });
  const more = page.getByRole("button", { name: "More" });
  // The strip's other controls arrive as the page loads and move both, so
  // compare them together until they settle.
  await expect
    .poll(async () => {
      const [s, m] = await Promise.all([search.boundingBox(), more.boundingBox()]);
      return s !== null && m !== null && m.x >= s.x + s.width;
    })
    .toBe(true);
});

test("Delete Entity needs an entity to delete", async ({ page }) => {
  await page.goto("/journals/");
  await page.getByRole("button", { name: "More" }).click();
  await expect(page.getByRole("menuitem", { name: "Delete Entity" })).toBeDisabled();
});

test("Delete Entity asks first, then leaves the page and deletes it", async ({ page }) => {
  await openEntity(page, "Loro");
  const more = page.getByRole("button", { name: "More" });
  await more.click();
  await page.getByRole("menuitem", { name: "Delete Entity" }).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toContainText("Delete “Loro”");

  // Cancel has the focus, and changes nothing.
  await expect(dialog.getByRole("button", { name: "Cancel" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Loro", level: 1 })).toBeVisible();

  await more.click();
  await page.getByRole("menuitem", { name: "Delete Entity" }).click();
  await dialog.getByRole("button", { name: "Delete" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page).not.toHaveURL(/\/entity\//);
  await expect(page.getByRole("heading", { name: "Loro", level: 1 })).toHaveCount(0);
});
