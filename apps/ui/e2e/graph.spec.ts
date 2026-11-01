import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures";

const svg = (page: Page) => page.getByRole("group", { name: "Graph of entities" });

async function openGraph(page: Page) {
  await page.locator("nav").getByRole("link", { name: "Graph", exact: true }).click();
  await expect(svg(page)).toBeVisible();
}

/** Where every node is, by name, as the SVG says. */
const placements = (page: Page): Promise<Record<string, string | null>> =>
  svg(page).locator("a").evaluateAll((nodes) =>
    Object.fromEntries(
      nodes.map((n) => [n.getAttribute("aria-label") ?? "", n.getAttribute("transform")]),
    ),
  );

/** Waits for the layout to stop moving. */
async function settled(page: Page) {
  await expect
    .poll(
      async () => {
        const before = JSON.stringify(await placements(page));
        await page.waitForTimeout(400);
        return before === JSON.stringify(await placements(page));
      },
      { timeout: 20_000 },
    )
    .toBe(true);
}

const centre = (transform: string | null | undefined) => {
  const [x = NaN, y = NaN] = /translate\(([^)]*)\)/.exec(transform ?? "")?.[1]?.split(",").map(Number) ?? [];
  return { x, y };
};

test("the graph is an entrypoint, and draws entities as links with lines between them", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await expect(page.locator("nav").getByRole("link", { name: "Graph", exact: true })).toHaveClass(/font-medium/);
  await expect(svg(page).getByRole("link", { name: "Loro", exact: true })).toBeVisible();
  await expect(svg(page).getByRole("link", { name: "Quiddity", exact: true })).toBeVisible();
  // Quiddity links to Loro in the made-up graph.
  expect(await svg(page).locator("line").count()).toBeGreaterThan(0);
});

test("a node opens its entity, and shift-click opens it at the side", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await svg(page).getByRole("link", { name: "iroh", exact: true }).click({ modifiers: ["Shift"] });
  await expect(page.locator("aside article").getByRole("heading", { name: "iroh" })).toBeVisible();
  await expect(svg(page)).toBeVisible();
});

test("going away and back finds every entity where it was", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await settled(page);
  const before = await placements(page);

  await svg(page).getByRole("link", { name: "Loro", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Loro", level: 1 })).toBeVisible();
  await openGraph(page);
  const after = await placements(page);

  expect(Object.keys(after).sort()).toEqual(Object.keys(before).sort());
  for (const [name, at] of Object.entries(before)) {
    const was = centre(at);
    const now = centre(after[name]);
    // Held still, not merely close.
    expect(Math.hypot(was.x - now.x, was.y - now.y), name).toBeLessThan(1);
  }
});

test("the pan and zoom are kept too, and a new entity joins without moving the rest", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await settled(page);
  const box = (await svg(page).boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(0, -400);
  await page.waitForTimeout(300);
  const layer = svg(page).locator("> g");
  const zoomed = await layer.getAttribute("transform");
  expect(zoomed).toMatch(/scale\((?!1\))/);
  await settled(page);
  const before = await placements(page);

  // Make an entity, then come back.
  await page.locator("nav").getByRole("link", { name: "All entities" }).click();
  await page.getByLabel("Name of a new entity").fill("Brand new");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Brand new", level: 1 })).toBeVisible();
  await openGraph(page);

  expect(await layer.getAttribute("transform")).toBe(zoomed);
  await expect(svg(page).getByRole("link", { name: "Brand new" })).toBeVisible();
  const after = await placements(page);
  for (const [name, at] of Object.entries(before)) {
    const was = centre(at);
    const now = centre(after[name]);
    expect(Math.hypot(was.x - now.x, was.y - now.y), name).toBeLessThan(1);
  }
});

const scale = (transform: string | null) => Number(/scale\(([^)]*)\)/.exec(transform ?? "")?.[1] ?? NaN);

/** Every number in a transform, in order: translate x, y, then scale. */
const numbers = (transform: string | null) => (transform?.match(/-?\d+(?:\.\d+)?(?:e-?\d+)?/gi) ?? []).map(Number);

/** Whether a node is wholly inside the drawing. */
async function inView(page: Page, name: string) {
  const box = (await svg(page).boundingBox())!;
  const at = (await svg(page).getByRole("link", { name, exact: true }).boundingBox())!;
  return at.x >= box.x && at.y >= box.y && at.x + at.width <= box.x + box.width && at.y + at.height <= box.y + box.height;
}

test("a first visit frames the linked entities", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await settled(page);
  // Quiddity links to Loro in the made-up graph.
  await expect.poll(() => inView(page, "Quiddity")).toBe(true);
  await expect.poll(() => inView(page, "Loro")).toBe(true);
});

test("Reset view frames everything again after panning and zooming", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await settled(page);
  const layer = svg(page).locator("> g");
  const framed = await layer.getAttribute("transform");

  const box = (await svg(page).boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(0, 1200);
  await expect.poll(() => layer.getAttribute("transform")).not.toBe(framed);

  await page.getByRole("button", { name: "Reset view" }).click();
  // The framing is computed from float positions, which can differ in the
  // last few places between runs, so compare to the nearest pixel.
  const want = numbers(framed);
  await expect
    .poll(async () => {
      const got = numbers(await layer.getAttribute("transform"));
      return got.length === want.length && got.every((n, i) => Math.abs(n - want[i]!) < 1);
    })
    .toBe(true);
});

test("Alt-click looks closer at a node and its neighbours, without opening it", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await settled(page);
  const layer = svg(page).locator("> g");
  const before = scale(await layer.getAttribute("transform"));
  const url = page.url();

  await svg(page).getByRole("link", { name: "iroh", exact: true }).click({ modifiers: ["Alt"] });
  await expect.poll(async () => scale(await layer.getAttribute("transform"))).toBeGreaterThan(before);
  await expect.poll(() => inView(page, "iroh")).toBe(true);
  expect(page.url()).toBe(url);
});
