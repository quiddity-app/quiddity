import { expect, test } from './fixtures';

import type { Page } from '@playwright/test';

const scrollTop = (page: Page) =>
  page.evaluate(() => document.querySelector('main')!.scrollTop);

/**
 * The history only learns of a place once the pane has drawn it, so a key
 * pressed before then has nothing to go back to: wait for the entity's title.
 */
const drawn = async (page: Page) => {
  await expect(page).toHaveURL(/\/entity\//);
  await expect(
    page.locator('main').getByRole('heading', { level: 1 }),
  ).toBeVisible();
};

/** Two frames, which is long enough for the pane's scroll event to have been heard. */
const settle = (page: Page) =>
  page.evaluate(
    () =>
      new Promise<void>((done) =>
        requestAnimationFrame(() =>
          requestAnimationFrame(() => {
            done();
          }),
        ),
      ),
  );

/**
 * Goes back with the keyboard. Before the history has seen where it started the
 * key does nothing, and a place is only seen after it is drawn, so press again
 * until it takes; pressing at the start of the history is harmless.
 */
const back = (page: Page) =>
  expect(async () => {
    await page.keyboard.press('Alt+ArrowLeft');
    await expect(page).toHaveURL(/\/all\/?$/, { timeout: 1000 });
  }).toPass({ timeout: 15_000 });

/** Opens All entities in a pane too short to hold it, scrolled down a little, and follows a link in view. */
async function leaveScrolled(page: Page) {
  await page.setViewportSize({ width: 1200, height: 300 });
  await page.goto('/all/');
  const links = page.locator('main').getByRole('link');
  await expect(links.first()).toBeVisible();
  // Scroll the first link near the top rather than by a fixed amount: how far
  // down the list starts depends on the size of the page's text.
  await page.evaluate(() => {
    const pane = document.querySelector('main')!;
    const link = pane.querySelector('a')!.getBoundingClientRect().top;
    const top = pane.getBoundingClientRect().top;
    pane.scrollTop += link - top - 40;
  });
  await settle(page);
  const scrolled = await scrollTop(page);
  expect(scrolled, 'the list needs to be taller than the pane').toBeGreaterThan(
    0,
  );
  // Playwright scrolls a link into view to click it, which would move the pane
  // after the position was read, so follow one that is already showing.
  await page.evaluate(() => {
    const pane = document.querySelector('main')!.getBoundingClientRect();
    const shown = [
      ...document.querySelectorAll<HTMLAnchorElement>('main a'),
    ].filter((a) => {
      const box = a.getBoundingClientRect();
      return box.top >= pane.top && box.bottom <= pane.bottom;
    });
    shown.at(-1)!.click();
  });
  await expect(page).not.toHaveURL(/\/all\/?$/);
  await drawn(page);
  return scrolled;
}

test('going back with the keyboard finds the page where it was left', async ({
  page,
}) => {
  const scrolled = await leaveScrolled(page);
  expect(
    await scrollTop(page),
    'a page not seen before starts at the top',
  ).toBe(0);
  await back(page);
  await expect.poll(() => scrollTop(page)).toBe(scrolled);
});

test("going back with the browser's button finds the page where it was left", async ({
  page,
}) => {
  const scrolled = await leaveScrolled(page);
  await page.goBack();
  await expect(page).toHaveURL(/\/all\/?$/);
  await expect.poll(() => scrollTop(page)).toBe(scrolled);
});

test('scrolling again after coming back is remembered, not undone', async ({
  page,
}) => {
  await leaveScrolled(page);
  await back(page);
  // The person's own scroll, which the restoring must not fight.
  await page.mouse.move(600, 150);
  await page.mouse.wheel(0, -500);
  await expect.poll(() => scrollTop(page)).toBe(0);
  // A different place from the one left before, so it is plainly a new visit.
  // Clicked from the page: Playwright would scroll the link into view first.
  await page.evaluate(() => {
    document.querySelectorAll<HTMLAnchorElement>('main a')[1]!.click();
  });
  await drawn(page);
  await back(page);
  await page.waitForTimeout(1200);
  expect(await scrollTop(page)).toBe(0);
});
