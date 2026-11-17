import type { Locator } from "@playwright/test";
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

test("an inverse tree indents each level, whether or not a row expands", async ({ page }) => {
  const left = async (link: Locator) => (await link.boundingBox())!.x;

  // CRDT has Loro under it, so it can be expanded; Loro has nothing under it.
  await openEntity(page, "data structure");
  const tree = page.getByRole("region", { name: "kinds" });
  const parent = await left(tree.getByRole("link", { name: "CRDT" }));
  const child = await left(tree.getByRole("link", { name: "Loro" }));
  expect(child, "a child sits to the right of its parent").toBeGreaterThan(parent + 8);

  // On CRDT's page Loro is at the top of the tree with no children: it should
  // start where CRDT did, which has a disclosure marker, not where the marker is.
  await openEntity(page, "CRDT");
  const leaf = await left(
    page.getByRole("region", { name: "kinds" }).getByRole("link", { name: "Loro" }),
  );
  expect(leaf).toBeCloseTo(parent, 0);
});

test("meta-properties read as text, and what doesn't fit is flagged", async ({ page }) => {
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
