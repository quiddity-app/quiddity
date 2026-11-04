import { expect, test, openEntity } from "./fixtures";

test("an entity shows what it inherits, and where from", async ({ page }) => {
  await openEntity(page, "Loro");
  const inherited = page.getByRole("region", { name: "Inherited" });
  const merges = inherited.getByRole("listitem").filter({ hasText: "merges" });
  await expect(merges).toContainText("without coordination");
  await merges.getByRole("link", { name: "CRDT" }).click();
  await expect(page.getByRole("heading", { name: "CRDT", level: 1 })).toBeVisible();
});

test("an inverse is a tree of what refers to the entity", async ({ page }) => {
  await openEntity(page, "data structure");
  const kinds = page.getByRole("region", { name: "kinds" });
  await expect(kinds.getByRole("link", { name: "CRDT" })).toBeVisible();
  await expect(kinds.getByRole("link", { name: "Loro" })).toBeVisible();
});

test("meta-properties are chips, and what doesn't fit is flagged", async ({ page }) => {
  await openEntity(page, "kind of");
  const props = page.locator("main article > ul > li").first();
  await expect(props).toContainText("##transitive");
  await expect(props).toContainText("##inverse");
  await expect(props.locator("[data-flagged]")).toHaveCount(0);

  // Give the property a meta-property Quiddity doesn't know.
  await props.locator("[data-block-text]").click();
  await page.keyboard.press("Control+End");
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("##colour");
  await page.keyboard.press("Escape");
  const flagged = props.locator("[data-flagged]");
  await expect(flagged).toHaveCount(1);
  await expect(flagged).toHaveAttribute("title", "Not a meta-property Quiddity knows");
});
