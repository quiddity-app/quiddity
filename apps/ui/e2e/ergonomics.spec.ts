import { expect, test, openEntity } from "./fixtures";

// How the app behaves for someone who isn't using a mouse, or doesn't want
// motion: a focus ring, targets big enough for a finger, and nothing that is
// only there on hover.

test("keyboard focus draws a solid ring, which the browser's own would not", async ({ page }) => {
  await page.goto("/devices/");
  await expect(page.getByRole("heading", { name: "Devices", level: 1 })).toBeVisible();
  await page.keyboard.press("Tab");
  const ring = await page.evaluate(() => {
    const style = getComputedStyle(document.activeElement!);
    return { style: style.outlineStyle, width: style.outlineWidth, offset: style.outlineOffset };
  });
  expect(ring).toEqual({ style: "solid", width: "2px", offset: "2px" });
});

test("the add-block row shows on hover with a mouse", async ({ page }) => {
  await openEntity(page, "Reading list");
  const add = page.locator("main").getByRole("button", { name: "+ Add a block" });
  await expect(add).toHaveCSS("opacity", "0");
  await add.hover();
  await expect(add).toHaveCSS("opacity", "1");
});

test("nothing moves when the person asks for less motion", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/devices/");
  await expect(page.locator("aside").first()).toHaveCSS("transition-duration", "0s");
});

test.describe("on a touch screen", () => {
  test.use({ hasTouch: true, isMobile: true });

  test("the add-block row is there without hover", async ({ page }) => {
    await openEntity(page, "Reading list");
    const add = page.locator("main").getByRole("button", { name: "+ Add a block" });
    await expect(add).toHaveCSS("opacity", "1");
  });

  test("buttons and rows are at least 44px tall", async ({ page }) => {
    await page.goto("/all/");
    const heights = await Promise.all(
      [
        page.locator("main").getByRole("button", { name: "Create", exact: true }),
        page.getByRole("button", { name: /^Search/ }),
        page.getByRole("link", { name: "Devices", exact: true }),
      ].map(async (target) => (await target.boundingBox())!.height),
    );
    for (const height of heights) expect(height).toBeGreaterThanOrEqual(44);
  });
});
