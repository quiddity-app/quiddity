import { expect, test } from './fixtures';

import type { Locator, Page } from '@playwright/test';

// The graph is drawn on a canvas, with a real link for each entity beside it
// for keyboards and screen readers. The links aren't positioned; they carry
// `data-x` and `data-y` (in the graph's own space) once the layout is still,
// and the canvas carries `data-view` (its pan and zoom) and `data-settled`.

const graph = (page: Page) =>
  page.getByRole('group', { name: 'Graph of entities' });
const canvas = (page: Page) => graph(page).locator('canvas');
const node = (page: Page, name: string) =>
  graph(page).getByRole('link', { name, exact: true });

async function openGraph(page: Page) {
  await page
    .locator('nav')
    .getByRole('link', { name: 'Graph', exact: true })
    .click();
  // The canvas isn't mounted until the replica has loaded (the page suspends
  // reading it), which can take a while on a busy runner.
  await expect(canvas(page)).toBeVisible({ timeout: 20_000 });
}

/** Waits for the layout to stop moving, and any framing to finish. */
async function settled(page: Page) {
  await expect(canvas(page)).toHaveAttribute('data-settled', 'true', {
    timeout: 20_000,
  });
  await steady(page);
}

/** Waits for the pan and zoom to stop changing. */
async function steady(page: Page) {
  await expect
    .poll(
      async () => {
        const before = await canvas(page).getAttribute('data-view');
        await page.waitForTimeout(150);
        return (
          before !== null &&
          before === (await canvas(page).getAttribute('data-view'))
        );
      },
      { timeout: 10_000 },
    )
    .toBe(true);
}

const view = async (page: Page) => {
  const [x = NaN, y = NaN, k = NaN] = (
    (await canvas(page).getAttribute('data-view')) ?? ''
  )
    .split(',')
    .map(Number);
  return { x, y, k };
};

/** Where every node is, by name, in the graph's own space. */
const placements = (
  page: Page,
): Promise<Record<string, { x: number; y: number }>> =>
  graph(page)
    .locator('a')
    .evaluateAll((nodes) =>
      Object.fromEntries(
        nodes.map((n) => [
          n.getAttribute('aria-label') ?? '',
          {
            x: Number(n.getAttribute('data-x')),
            y: Number(n.getAttribute('data-y')),
          },
        ]),
      ),
    );

/** Where a node is within the canvas, as the person sees it. */
async function onScreen(page: Page, name: string) {
  const { x, y, k } = await view(page);
  const at = await node(page, name).evaluate((n) => ({
    x: Number(n.getAttribute('data-x')),
    y: Number(n.getAttribute('data-y')),
  }));
  return { x: x + at.x * k, y: y + at.y * k };
}

async function clickNode(
  page: Page,
  name: string,
  modifiers: Parameters<Locator['click']>[0] = {},
) {
  await canvas(page).click({
    ...modifiers,
    position: await onScreen(page, name),
  });
}

async function inView(page: Page, name: string) {
  const box = (await canvas(page).boundingBox())!;
  const { x, y } = await onScreen(page, name);
  return x >= 0 && y >= 0 && x <= box.width && y <= box.height;
}

test('the graph is an entrypoint, and has a link for each entity', async ({
  page,
}) => {
  await page.goto('/');
  await openGraph(page);
  await expect(
    page.locator('nav').getByRole('link', { name: 'Graph', exact: true }),
  ).toHaveClass(/font-medium/);
  await expect(node(page, 'Loro')).toBeAttached();
  await expect(node(page, 'Quiddity')).toBeAttached();
  // The canvas is what is seen, so the links add nothing for a screen reader to repeat.
  await expect(canvas(page)).toHaveAttribute('aria-hidden', 'true');
});

test('a node opens its entity, and shift-click opens it at the side', async ({
  page,
}) => {
  await page.goto('/');
  await openGraph(page);
  await settled(page);
  await clickNode(page, 'iroh', { modifiers: ['Shift'] });
  await expect(
    page.locator('aside article').getByRole('heading', { name: 'iroh' }),
  ).toBeVisible();
  await expect(canvas(page)).toBeVisible();
});

test('a keyboard can follow the links', async ({ page }) => {
  await page.goto('/');
  await openGraph(page);
  await node(page, 'Loro').focus();
  await page.keyboard.press('Enter');
  await expect(
    page.getByRole('heading', { name: 'Loro', level: 1 }),
  ).toBeVisible();
});

test('going away and back finds every entity where it was', async ({
  page,
}) => {
  await page.goto('/');
  await openGraph(page);
  await settled(page);
  const before = await placements(page);

  await clickNode(page, 'Loro');
  await expect(
    page.getByRole('heading', { name: 'Loro', level: 1 }),
  ).toBeVisible();
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

test('coming back draws the links, not only the entities', async ({ page }) => {
  await page.goto('/');
  await openGraph(page);
  await settled(page);
  const links = Number(await canvas(page).getAttribute('data-links'));
  expect(links).toBeGreaterThan(0);
  await expect(canvas(page)).toHaveAttribute('data-drawn', String(links));

  await clickNode(page, 'Loro');
  await expect(
    page.getByRole('heading', { name: 'Loro', level: 1 }),
  ).toBeVisible();
  await openGraph(page);
  await settled(page);
  // Every entity has a remembered place now, so nothing is laid out afresh.
  await expect(canvas(page)).toHaveAttribute('data-drawn', String(links));
});

test('the pan and zoom are kept too, and a new entity joins without moving the rest', async ({
  page,
}) => {
  await page.goto('/');
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

  // Make an entity by linking to it from today's journal, then come back.
  await page
    .locator('nav')
    .getByRole('link', { name: 'Journals', exact: true })
    .click();
  const today = page.locator('main section').first();
  await today.locator('[data-block-text]').last().click();
  await page.keyboard.press('End');
  await page.keyboard.press('Enter');
  await page.keyboard.type('[[Brand new]]');
  await page.keyboard.press('Escape');
  await expect(today.getByRole('link', { name: 'Brand new' })).toBeVisible();
  await openGraph(page);
  // The new entity is linked only from a journal, so it is an orphan unless
  // journals are shown. They weren't before, so none of them is compared.
  await option(page, 'Journal pages').check();
  await settled(page);

  expect(await view(page)).toEqual(zoomed);
  await expect(node(page, 'Brand new')).toBeAttached();
  const after = await placements(page);
  for (const [name, was] of Object.entries(before)) {
    const now = after[name]!;
    expect(Math.hypot(was.x - now.x, was.y - now.y), name).toBeLessThan(1);
  }
});

test("dragging a node moves it, and doesn't open it", async ({ page }) => {
  await page.goto('/');
  await openGraph(page);
  await settled(page);
  const url = page.url();
  const was = await onScreen(page, 'iroh');
  const box = (await canvas(page).boundingBox())!;
  await page.mouse.move(box.x + was.x, box.y + was.y);
  await page.mouse.down();
  await page.mouse.move(box.x + was.x + 80, box.y + was.y + 60, { steps: 8 });
  // The node stays where it was grabbed, relative to the pointer: it doesn't
  // jump on the first move. (`data-grabbed` is where the drag has it.)
  const [gx = NaN, gy = NaN] = (
    (await canvas(page).getAttribute('data-grabbed')) ?? ''
  )
    .split(',')
    .map(Number);
  expect(Math.hypot(gx - (was.x + 80), gy - (was.y + 60))).toBeLessThan(1);
  await page.mouse.up();
  await settled(page);
  expect(page.url()).toBe(url);
  // The layout settles once let go, so the node is near where it was dropped, not at it.
  const now = await onScreen(page, 'iroh');
  expect(Math.hypot(now.x - was.x, now.y - was.y)).toBeGreaterThan(10);
});

test('a first visit frames the linked entities', async ({ page }) => {
  await page.goto('/');
  await openGraph(page);
  await settled(page);
  // Quiddity links to Loro in the made-up graph.
  expect(await inView(page, 'Quiddity')).toBe(true);
  expect(await inView(page, 'Loro')).toBe(true);
});

test('Reset view frames everything again after panning and zooming', async ({
  page,
}) => {
  await page.goto('/');
  await openGraph(page);
  await settled(page);
  const framed = await view(page);

  const box = (await canvas(page).boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(0, 1200);
  await expect.poll(async () => (await view(page)).k).not.toBe(framed.k);

  await page.getByRole('button', { name: 'Reset view' }).click();
  await expect.poll(() => view(page)).toEqual(framed);
});

test('Alt-click looks closer at a node and its neighbours, without opening it', async ({
  page,
}) => {
  await page.goto('/');
  await openGraph(page);
  await settled(page);
  const before = (await view(page)).k;
  const url = page.url();

  await clickNode(page, 'iroh', { modifiers: ['Alt'] });
  await expect.poll(async () => (await view(page)).k).toBeGreaterThan(before);
  await steady(page);
  expect(await inView(page, 'iroh')).toBe(true);
  expect(page.url()).toBe(url);
});

test('labels are dropped as the graph is zoomed out, and come back when it is zoomed in', async ({
  page,
}) => {
  await page.goto('/');
  await openGraph(page);
  await settled(page);
  const labels = async () =>
    Number(await canvas(page).getAttribute('data-labels'));
  const box = (await canvas(page).boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);

  // Framed, the whole graph is smaller than life and nothing is yet big enough
  // to name, so zoom in until some labels show before testing that they go.
  for (let i = 0; i < 40; i++) await page.mouse.wheel(0, -400);
  await expect.poll(labels).toBeGreaterThan(0);
  const zoomedIn = await labels();

  for (let i = 0; i < 60; i++) await page.mouse.wheel(0, 400);
  await expect.poll(labels).toBeLessThan(zoomedIn);
  const zoomedOut = await labels();

  for (let i = 0; i < 60; i++) await page.mouse.wheel(0, -400);
  await expect.poll(labels).toBeGreaterThan(zoomedOut);
});

test.describe('on a dense display', () => {
  test.use({ deviceScaleFactor: 2 });

  test('the canvas has twice the pixels, but the view is in CSS pixels', async ({
    page,
  }) => {
    await page.goto('/');
    await openGraph(page);
    await settled(page);
    const size = await canvas(page).evaluate((c: HTMLCanvasElement) => ({
      backing: c.width,
      css: c.getBoundingClientRect().width,
    }));
    expect(size.backing).toBe(Math.round(size.css * 2));
    // Framing uses the CSS size, so the graph fits the same as on any display.
    const { x } = await view(page);
    expect(x).toBeLessThan(size.css);
  });
});

const options = (page: Page) =>
  graph(page).getByRole('group', { name: 'Graph options' });
const option = (page: Page, name: string) =>
  options(page).getByRole('checkbox', { name });
const today = () => {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};

test('property links, journal pages and orphan pages are left out to begin with', async ({
  page,
}) => {
  await page.goto('/');
  await openGraph(page);
  for (const name of ['Property links', 'Journal pages', 'Orphan pages']) {
    await expect(option(page, name)).not.toBeChecked();
  }
  await expect(canvas(page)).toHaveAttribute('data-weak', '0');
  await expect(node(page, today())).toHaveCount(0);
  await expect(node(page, 'Reading list')).toHaveCount(0);
  // Linked from the journals as well as itself linking on, so it stays.
  await expect(node(page, 'Quiddity')).toBeAttached();
});

test('each option brings back what it names, and only that', async ({
  page,
}) => {
  await page.goto('/');
  await openGraph(page);
  const links = async () =>
    Number(await canvas(page).getAttribute('data-links'));
  const before = await links();

  await option(page, 'Property links').check();
  await expect
    .poll(async () => Number(await canvas(page).getAttribute('data-weak')))
    .toBeGreaterThan(0);
  expect(await links()).toBeGreaterThan(before);
  await expect(node(page, today())).toHaveCount(0);
  await expect(node(page, 'Reading list')).toHaveCount(0);
  await option(page, 'Property links').uncheck();
  await expect(canvas(page)).toHaveAttribute('data-weak', '0');

  await option(page, 'Orphan pages').check();
  await expect(node(page, 'Reading list')).toBeAttached();
  await expect(node(page, today())).toHaveCount(0);
  await option(page, 'Orphan pages').uncheck();
  await expect(node(page, 'Reading list')).toHaveCount(0);

  await option(page, 'Journal pages').check();
  await expect(node(page, today())).toBeAttached();
  await expect(node(page, 'Reading list')).toHaveCount(0);
});

test('orphans are judged after journals are hidden', async ({ page }) => {
  await page.goto('/');
  await openGraph(page);
  // Pages only journals link to are orphans once the journals are hidden.
  await option(page, 'Journal pages').check();
  const withJournals = await graph(page).locator('a').count();
  await option(page, 'Journal pages').uncheck();
  expect(await graph(page).locator('a').count()).toBeLessThan(withJournals);
  await expect(node(page, 'Quiddity')).toBeAttached();
});

// What is left when nothing is new to place used to lose its lines: the
// simulation was only told of the links when something had to be placed.
test('hiding what an option showed leaves the other links drawn', async ({
  page,
}) => {
  await page.goto('/');
  await openGraph(page);
  const links = async () =>
    Number(await canvas(page).getAttribute('data-links'));
  const before = await links();
  expect(before).toBeGreaterThan(0);

  await option(page, 'Journal pages').check();
  await expect.poll(links).toBeGreaterThan(before);
  await option(page, 'Journal pages').uncheck();
  await expect.poll(links).toBe(before);
});

test('the options panel can be hidden and shown, and keeps its choices', async ({
  page,
}) => {
  await page.goto('/');
  await openGraph(page);
  await option(page, 'Orphan pages').check();
  const toggle = graph(page).getByRole('button', { name: 'Hide options' });
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await toggle.click();
  await expect(options(page)).toBeHidden();
  // Hiding the panel doesn't undo what it chose.
  await expect(node(page, 'Reading list')).toBeAttached();
  await graph(page).getByRole('button', { name: 'Options' }).click();
  await expect(option(page, 'Orphan pages')).toBeChecked();
});
