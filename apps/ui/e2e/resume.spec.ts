import { expect, test } from './fixtures';

import type { Page } from '@playwright/test';

/** The page as the app's webview, whose graph keeps its IDs from load to load. */
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    Object.assign(window, { __quiddityResume: true });
  });
});

/** What Android does to a page going to the background. */
const background = (page: Page) =>
  page.evaluate(() => window.dispatchEvent(new Event('pagehide')));

const scrollTop = (page: Page) =>
  page.evaluate(() => document.querySelector('main')!.scrollTop);

test('a page loaded again at / goes back to where it was left', async ({
  page,
}) => {
  await page.goto('/all/');
  await expect(page.locator('main').getByRole('link').first()).toBeVisible();
  await background(page);

  await page.goto('/');
  await expect(page).toHaveURL(/\/all\/$/);
});

test('it is scrolled as it was', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 300 });
  await page.goto('/all/');
  await expect(page.locator('main').getByRole('link').first()).toBeVisible();
  await page.locator('main').hover();
  await page.mouse.wheel(0, 120);
  await expect.poll(() => scrollTop(page)).toBeGreaterThan(50);
  const left = await scrollTop(page);
  await background(page);

  await page.goto('/');
  await expect(page).toHaveURL(/\/all\/$/);
  await expect.poll(() => scrollTop(page)).toBe(left);
});

test('a page left at / has nothing to go back to, so it goes home', async ({
  page,
}) => {
  await page.goto('/');
  await expect(page).toHaveURL(/\/journals\/$/);
  const home = page.url();
  await background(page);
  await page.goto('/');
  await expect(page).toHaveURL(home);
});

test('without the app, a page loaded at / goes home', async ({ browser }) => {
  // A new context has neither the init script nor what the last one kept.
  const page = await (await browser.newContext()).newPage();
  await page.goto('/all/');
  await expect(page.locator('main').getByRole('link').first()).toBeVisible();
  await background(page);
  await page.goto('/');
  await expect(page).not.toHaveURL(/\/all\//);
});

test('changing graph forgets the place, and keeps none on the way out', async ({
  page,
}) => {
  await page.goto('/all/?graphs');
  await expect(page.locator('main').getByRole('link').first()).toBeVisible();
  await background(page);
  await page.getByRole('button', { name: 'Graph: Quiddity' }).first().click();
  await page.getByRole('menuitem', { name: 'New empty graph…' }).click();
  const dialog = page.getByRole('dialog', { name: 'New empty graph' });
  await dialog.getByLabel('Name').fill('Work');
  await dialog.getByRole('button', { name: 'Create' }).click();
  await expect(
    page.getByRole('button', { name: 'Graph: Work' }).first(),
  ).toBeVisible();
  // The page started over at `/`, and wasn't put back where the old graph was.
  await expect(page).not.toHaveURL(/\/all\//);
});
