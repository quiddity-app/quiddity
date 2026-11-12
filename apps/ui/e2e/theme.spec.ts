import { expect, test, editor, openEntity } from "./fixtures";

// What follows the theme without a component spelling a colour: the floating
// layers, and the graph's canvas, which can't take a colour from a class.

/** A shadow's visible layers: Tailwind adds empty ones for its ring and inset shadows. */
const visible = (shadow: string) => shadow.replace(/rgba\(0, 0, 0, 0\) 0px 0px 0px 0px(, )?/g, "");

test("the link list and the search panel are the same kind of layer", async ({ page }) => {
  await openEntity(page, "Quiddity");
  await page.locator("main article > ul").getByText("Local-first").click();
  await page.keyboard.press("End");
  await page.keyboard.type(" [[Lo");
  const list = page.locator(".cm-tooltip.cm-link-completions");
  await expect(list).toBeVisible();
  const own = await list.evaluate((el) => {
    const s = getComputedStyle(el);
    return { shadow: s.boxShadow, radius: s.borderTopLeftRadius, ground: s.backgroundColor, edge: s.borderTopColor };
  });
  await page.keyboard.press("Escape");
  await expect(editor(page)).toHaveCount(0);

  await page.keyboard.press("Control+k");
  const panel = page.getByRole("dialog");
  await expect(panel).toBeVisible();
  const search = await panel.evaluate((el) => {
    const s = getComputedStyle(el);
    return { shadow: s.boxShadow, radius: s.borderTopLeftRadius, ground: s.backgroundColor, edge: s.borderTopColor };
  });
  expect({ ...own, shadow: visible(own.shadow) }).toEqual({ ...search, shadow: visible(search.shadow) });
});

test("the graph is painted in the theme's colour for a node", async ({ page }) => {
  await page.goto("/");
  await page.locator("nav").getByRole("link", { name: "Graph", exact: true }).click();
  const graph = page.getByRole("group", { name: "Graph of entities" });
  const canvas = graph.locator("canvas");
  await expect(canvas).toHaveAttribute("data-settled", "true", { timeout: 20_000 });
  await page.waitForTimeout(400);

  // The theme says nodes are red, and the page is told the scheme changed.
  await page.evaluate(() => document.documentElement.style.setProperty("--graph-node", "rgb(255, 0, 0)"));
  await page.emulateMedia({ colorScheme: "dark" });

  // The view, the node and the pixels are read together, and again on every
  // poll: the graph may still be gliding to its frame, and a position read once
  // would be sampled after the node had moved on. A node is at least 3px
  // across, so a 3x3 window around its centre is all fill, except where another
  // node's label has been drawn over it: the layout starts from random places,
  // so now and then a glyph lands on the centre and a single pixel there is
  // blended with the label's colour. Any pixel of the window that is the
  // theme's colour will do.
  await expect
    .poll(() =>
      page.evaluate(() => {
        const group = document.querySelector('[role="group"][aria-label="Graph of entities"]')!;
        const canvas = group.querySelector("canvas")!;
        const node = [...group.querySelectorAll("a")].find((a) => a.getAttribute("aria-label") === "Loro")!;
        const [x = 0, y = 0, k = 1] = (canvas.getAttribute("data-view") ?? "").split(",").map(Number);
        const ratio = canvas.width / canvas.clientWidth;
        const at = { x: x + Number(node.getAttribute("data-x")) * k, y: y + Number(node.getAttribute("data-y")) * k };
        const window = canvas
          .getContext("2d")!
          .getImageData(Math.round(at.x * ratio) - 1, Math.round(at.y * ratio) - 1, 3, 3).data;
        const pixels = Array.from({ length: 9 }, (_, i) => [...window.slice(i * 4, i * 4 + 4)]);
        return pixels.find(([r, g, b, a]) => r === 255 && g === 0 && b === 0 && a === 255) ?? pixels[4];
      }),
    )
    .toEqual([255, 0, 0, 255]);
});

/** "#f8fafc" as the browser writes a colour: "rgb(248, 250, 252)". */
const rgb = (hex: string) => {
  const [r, g, b] = [1, 3, 5].map((at) => parseInt(hex.slice(at, at + 2), 16));
  return `rgb(${r}, ${g}, ${b})`;
};

// What the browser, or the system around a webview, paints outside the page
// (a tab strip, a status bar) is told the page's own ground, in each scheme.
for (const scheme of ["light", "dark"] as const) {
  test(`the page names its ground to the system in ${scheme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.goto("/devices/");
    const ground = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    const named = await page
      .locator('meta[name="theme-color"]')
      .evaluateAll((metas) =>
        metas.map((m) => ({ media: m.getAttribute("media"), colour: m.getAttribute("content") ?? "" })),
      );
    const mine = named.find((m) => m.media === `(prefers-color-scheme: ${scheme})`);
    expect(mine, "a theme-color for this scheme").toBeDefined();
    expect(rgb(mine!.colour)).toBe(ground);
  });
}
