import { expect, test, editor, openEntity } from './fixtures';

import type { Page } from '@playwright/test';

/** Plays an on-screen keyboard of this height, as the visual viewport shrinks. */
const keyboard = (page: Page, height: number) =>
  page.evaluate((h) => {
    const visual = window.visualViewport!;
    Object.defineProperty(visual, 'height', {
      configurable: true,
      get: () => window.innerHeight - h,
    });
    visual.dispatchEvent(new Event('resize'));
  }, height);

const LEAF = 'Shift-click a link to open it at the side.';

test.beforeEach(async ({ page }) => {
  await openEntity(page, 'Quiddity');
});

test('the toolbar is there only while editing with an on-screen keyboard', async ({
  page,
}) => {
  const toolbar = page.getByRole('toolbar', { name: 'Block actions' });
  await page
    .locator('main article > ul')
    .getByText(LEAF, { exact: true })
    .click();
  await expect(editor(page)).toBeFocused();
  await expect(toolbar).toHaveCount(0);
  await keyboard(page, 300);
  await expect(toolbar).toBeVisible();
  await keyboard(page, 0);
  await expect(toolbar).toHaveCount(0);
});

test('its buttons act on the block and keep it being edited', async ({
  page,
}) => {
  await page
    .locator('main article > ul')
    .getByText(LEAF, { exact: true })
    .click();
  await keyboard(page, 300);
  const toolbar = page.getByRole('toolbar', { name: 'Block actions' });
  await toolbar.getByRole('button', { name: 'Outdent' }).click();
  await expect(page.locator('main [role=status]').first()).toHaveText(
    'Outdented',
  );
  await expect(editor(page)).toBeFocused();
  await toolbar.getByRole('button', { name: 'Insert link' }).click();
  await expect(editor(page)).toContainText('[[]]');
  await page.keyboard.type('Loro');
  await expect(editor(page)).toContainText('[[Loro]]');
});

test('the pane pads itself, and its scroll, by what covers its foot', async ({
  page,
}) => {
  await page
    .locator('main article > ul')
    .getByText(LEAF, { exact: true })
    .click();
  const padding = () =>
    page.locator('main').evaluate((m) => {
      const style = getComputedStyle(m);
      return [style.paddingBottom, style.scrollPaddingBottom];
    });
  expect(await padding()).toEqual(['0px', '0px']);
  await keyboard(page, 300);
  const toolbar = await page
    .getByRole('toolbar', { name: 'Block actions' })
    .evaluate((t) => (t as HTMLElement).offsetHeight);
  const space = `${300 + toolbar}px`;
  await expect.poll(padding).toEqual([space, space]);
  await keyboard(page, 0);
  await expect.poll(padding).toEqual(['0px', '0px']);
});
