import { expect, test } from "./fixtures";

// The type and the page widths: one size and spacing for a page's title, one
// reading width for text, and the monospace face that is bundled with the app.

for (const [path, name] of [
  ["/all/", "All entities"],
  ["/devices/", "Devices"],
  ["/journals/", "Journals"],
] as const) {
  test(`${name} has a page title of the common size and spacing`, async ({ page }) => {
    await page.goto(path);
    const title = page.getByRole("heading", { name, level: 1 });
    await expect(title).toHaveCSS("font-size", "24px");
    await expect(title).toHaveCSS("margin-bottom", "16px");
  });
}

test("a reading page is the reading width, and the table page the wide one", async ({ page }) => {
  await page.goto("/journals/");
  await expect(page.locator("main article").first()).toHaveCSS("max-width", "800px");
  await page.goto("/all/");
  await expect(page.locator("main article").first()).toHaveCSS("max-width", "1152px");
});

test("monospace text is set in the bundled face, which has no ligatures", async ({ page }) => {
  await page.goto("/devices/");
  const loaded = await page.evaluate(async () => {
    const faces = await document.fonts.load('1em "JetBrains Mono NL"');
    return faces.map((face) => face.status);
  });
  expect(loaded).toEqual(["loaded"]);
  const family = await page.evaluate(() => {
    const probe = document.createElement("code");
    probe.className = "font-mono";
    document.body.append(probe);
    return getComputedStyle(probe).fontFamily;
  });
  expect(family.startsWith('"JetBrains Mono NL"')).toBe(true);
});
