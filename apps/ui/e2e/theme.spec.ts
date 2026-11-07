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

  const [x = 0, y = 0, k = 1] = ((await canvas.getAttribute("data-view")) ?? "").split(",").map(Number);
  const at = await graph.getByRole("link", { name: "Loro", exact: true }).evaluate((n) => ({
    x: Number(n.getAttribute("data-x")),
    y: Number(n.getAttribute("data-y")),
  }));
  await expect
    .poll(() =>
      canvas.evaluate(
        (c: HTMLCanvasElement, p) => {
          const ratio = c.width / c.clientWidth;
          return [...c.getContext("2d")!.getImageData(Math.round(p.x * ratio), Math.round(p.y * ratio), 1, 1).data];
        },
        { x: x + at.x * k, y: y + at.y * k },
      ),
    )
    .toEqual([255, 0, 0, 255]);
});
