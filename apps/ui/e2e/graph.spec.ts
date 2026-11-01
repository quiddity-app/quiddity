import type { Locator, Page } from "@playwright/test";
import { expect, test } from "./fixtures";

// The graph is drawn on a canvas, with a real link for each entity beside it
// for keyboards and screen readers. The links aren't positioned; they carry
// `data-x` and `data-y` (in the graph's own space) once the layout is still,
// and the canvas carries `data-view` (its pan and zoom) and `data-settled`.

const graph = (page: Page) => page.getByRole("group", { name: "Graph of entities" });
const canvas = (page: Page) => graph(page).locator("canvas");
const node = (page: Page, name: string) => graph(page).getByRole("link", { name, exact: true });

async function openGraph(page: Page) {
  await page.locator("nav").getByRole("link", { name: "Graph", exact: true }).click();
  await expect(canvas(page)).toBeVisible();
}

/** Waits for the layout to stop moving, and any framing to finish. */
async function settled(page: Page) {
  await expect(canvas(page)).toHaveAttribute("data-settled", "true", { timeout: 20_000 });
  await steady(page);
}

/** Waits for the pan and zoom to stop changing. */
async function steady(page: Page) {
  await expect
    .poll(
      async () => {
        const before = await canvas(page).getAttribute("data-view");
        await page.waitForTimeout(150);
        return before !== null && before === (await canvas(page).getAttribute("data-view"));
      },
      { timeout: 10_000 },
    )
    .toBe(true);
}

const view = async (page: Page) => {
  const [x = NaN, y = NaN, k = NaN] = ((await canvas(page).getAttribute("data-view")) ?? "").split(",").map(Number);
  return { x, y, k };
};

/** Where every node is, by name, in the graph's own space. */
const placements = (page: Page): Promise<Record<string, { x: number; y: number }>> =>
  graph(page)
    .locator("a")
    .evaluateAll((nodes) =>
      Object.fromEntries(
        nodes.map((n) => [
          n.getAttribute("aria-label") ?? "",
          { x: Number(n.getAttribute("data-x")), y: Number(n.getAttribute("data-y")) },
        ]),
      ),
    );

/** Where a node is within the canvas, as the person sees it. */
async function onScreen(page: Page, name: string) {
  const { x, y, k } = await view(page);
  const at = await node(page, name).evaluate((n) => ({
    x: Number(n.getAttribute("data-x")),
    y: Number(n.getAttribute("data-y")),
  }));
  return { x: x + at.x * k, y: y + at.y * k };
}

async function clickNode(page: Page, name: string, modifiers: Parameters<Locator["click"]>[0] = {}) {
  await canvas(page).click({ ...modifiers, position: await onScreen(page, name) });
}

async function inView(page: Page, name: string) {
  const box = (await canvas(page).boundingBox())!;
  const { x, y } = await onScreen(page, name);
  return x >= 0 && y >= 0 && x <= box.width && y <= box.height;
}

test("the graph is an entrypoint, and has a link for each entity", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await expect(page.locator("nav").getByRole("link", { name: "Graph", exact: true })).toHaveClass(/font-medium/);
  await expect(node(page, "Loro")).toBeAttached();
  await expect(node(page, "Quiddity")).toBeAttached();
  // The canvas is what is seen, so the links add nothing for a screen reader to repeat.
  await expect(canvas(page)).toHaveAttribute("aria-hidden", "true");
});

test("a node opens its entity, and shift-click opens it at the side", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await settled(page);
  await clickNode(page, "iroh", { modifiers: ["Shift"] });
  await expect(page.locator("aside article").getByRole("heading", { name: "iroh" })).toBeVisible();
  await expect(canvas(page)).toBeVisible();
});

test("a keyboard can follow the links", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await node(page, "Loro").focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Loro", level: 1 })).toBeVisible();
});

test("going away and back finds every entity where it was", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await settled(page);
  const before = await placements(page);

  await clickNode(page, "Loro");
  await expect(page.getByRole("heading", { name: "Loro", level: 1 })).toBeVisible();
  await openGraph(page);
  await settled(page);
  const after = await placements(page);

  expect(Object.keys(after).sort()).toEqual(Object.keys(before).sort());
  for (const [name, was] of Object.entries(before)) {
    const now = after[name]!;
    // Held still, not merely close.
    expect(Math.hypot(was.x - now.x, was.y - now.y), name).toBeLessThan(1);
  }
});

test("the pan and zoom are kept too, and a new entity joins without moving the rest", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await settled(page);
  const box = (await canvas(page).boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  const framed = await view(page);
  await page.mouse.wheel(0, -400);
  await expect.poll(async () => (await view(page)).k).toBeGreaterThan(framed.k);
  await steady(page);
  const zoomed = await view(page);
  const before = await placements(page);

  // Make an entity, then come back.
  await page.locator("nav").getByRole("link", { name: "All entities" }).click();
  await page.getByLabel("Name of a new entity").fill("Brand new");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Brand new", level: 1 })).toBeVisible();
  await openGraph(page);
  await settled(page);

  expect(await view(page)).toEqual(zoomed);
  await expect(node(page, "Brand new")).toBeAttached();
  const after = await placements(page);
  for (const [name, was] of Object.entries(before)) {
    const now = after[name]!;
    expect(Math.hypot(was.x - now.x, was.y - now.y), name).toBeLessThan(1);
  }
});

test("dragging a node moves it, and doesn't open it", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await settled(page);
  const url = page.url();
  const was = await onScreen(page, "iroh");
  const box = (await canvas(page).boundingBox())!;
  await page.mouse.move(box.x + was.x, box.y + was.y);
  await page.mouse.down();
  await page.mouse.move(box.x + was.x + 80, box.y + was.y + 60, { steps: 8 });
  // The node stays where it was grabbed, relative to the pointer: it doesn't
  // jump on the first move. (`data-grabbed` is where the drag has it.)
  const [gx = NaN, gy = NaN] = ((await canvas(page).getAttribute("data-grabbed")) ?? "").split(",").map(Number);
  expect(Math.hypot(gx - (was.x + 80), gy - (was.y + 60))).toBeLessThan(1);
  await page.mouse.up();
  await settled(page);
  expect(page.url()).toBe(url);
  // The layout settles once let go, so the node is near where it was dropped, not at it.
  const now = await onScreen(page, "iroh");
  expect(Math.hypot(now.x - was.x, now.y - was.y)).toBeGreaterThan(10);
});

test("a first visit frames the linked entities", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await settled(page);
  // Quiddity links to Loro in the made-up graph.
  expect(await inView(page, "Quiddity")).toBe(true);
  expect(await inView(page, "Loro")).toBe(true);
});

test("Reset view frames everything again after panning and zooming", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await settled(page);
  const framed = await view(page);

  const box = (await canvas(page).boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(0, 1200);
  await expect.poll(async () => (await view(page)).k).not.toBe(framed.k);

  await page.getByRole("button", { name: "Reset view" }).click();
  await expect.poll(() => view(page)).toEqual(framed);
});

test("Alt-click looks closer at a node and its neighbours, without opening it", async ({ page }) => {
  await page.goto("/");
  await openGraph(page);
  await settled(page);
  const before = (await view(page)).k;
  const url = page.url();

  await clickNode(page, "iroh", { modifiers: ["Alt"] });
  await expect.poll(async () => (await view(page)).k).toBeGreaterThan(before);
  await steady(page);
  expect(await inView(page, "iroh")).toBe(true);
  expect(page.url()).toBe(url);
});
