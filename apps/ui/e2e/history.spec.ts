import { expect, test, openEntity } from './fixtures';

const heading = (page: import('@playwright/test').Page, name: string) =>
  page.getByRole('heading', { name, level: 1 });

for (const { label, back, forward } of [
  {
    label: 'Alt with the arrows',
    back: 'Alt+ArrowLeft',
    forward: 'Alt+ArrowRight',
  },
  { label: 'the square brackets', back: 'Control+[', forward: 'Control+]' },
]) {
  test(`${label} step the main pane and leave the side panel alone`, async ({
    page,
  }) => {
    await openEntity(page, 'Quiddity');
    // Go somewhere else in the main pane, then open a card at the side.
    await page
      .locator('main article > ul')
      .getByRole('link', { name: 'Loro' })
      .first()
      .click();
    await expect(heading(page, 'Loro')).toBeVisible();
    await page.locator('nav').getByRole('link', { name: 'Journals' }).click();
    await expect(page).toHaveURL(/\/journals\/$/);
    await page
      .locator('main')
      .getByRole('link')
      .first()
      .click({ modifiers: ['Shift'] });
    await expect(page).toHaveURL(/open=/);
    const open = new URL(page.url()).searchParams.get('open');

    await page.keyboard.press(back);
    await expect(heading(page, 'Loro')).toBeVisible();
    expect(new URL(page.url()).searchParams.get('open')).toBe(open);

    await page.keyboard.press(back);
    await expect(heading(page, 'Quiddity')).toBeVisible();
    expect(new URL(page.url()).searchParams.get('open')).toBe(open);

    await page.keyboard.press(forward);
    await expect(heading(page, 'Loro')).toBeVisible();
    expect(new URL(page.url()).searchParams.get('open')).toBe(open);
  });
}

test("opening and closing a card isn't a step, so forward still works after one", async ({
  page,
}) => {
  const links = page.locator('main article > ul');
  await openEntity(page, 'Quiddity');
  await links.getByRole('link', { name: 'Loro' }).first().click();
  await expect(heading(page, 'Loro')).toBeVisible();
  await page.keyboard.press('Alt+ArrowLeft');
  await expect(heading(page, 'Quiddity')).toBeVisible();

  await links
    .getByRole('link', { name: 'Loro' })
    .first()
    .click({ modifiers: ['Shift'] });
  await expect(page).toHaveURL(/open=/);
  await page.getByRole('link', { name: 'Close all' }).click();
  await expect(page).not.toHaveURL(/open=/);

  await page.keyboard.press('Alt+ArrowRight');
  await expect(heading(page, 'Loro')).toBeVisible();
});

test("in an editor the keys stay with the editor and don't leave the page", async ({
  page,
}) => {
  await openEntity(page, 'Quiddity');
  await page
    .locator('main article > ul')
    .getByRole('link', { name: 'Loro' })
    .first()
    .click();
  await expect(heading(page, 'Loro')).toBeVisible();
  await page.locator('nav').getByRole('link', { name: 'Journals' }).click();
  await expect(page).toHaveURL(/\/journals\/$/);
  await page.locator('main [data-block-text]').first().click();
  const editor = page.locator('.cm-content').first();
  await expect(editor).toBeFocused();

  // Navigation is asynchronous, so a URL that hasn't moved yet proves nothing:
  // ask instead whether the page took the key from the editor.
  await page.evaluate(() => {
    const w = window as unknown as { taken: boolean[] };
    w.taken = [];
    window.addEventListener(
      'keydown',
      (e) => {
        // A chord also sends a keydown for its modifier.
        if (!['Alt', 'Control', 'Meta', 'Shift'].includes(e.key))
          w.taken.push(e.defaultPrevented);
      },
      true,
    );
  });
  for (const key of [
    'Alt+ArrowLeft',
    'Control+[',
    'Control+]',
    'Alt+ArrowRight',
  ]) {
    await page.keyboard.press(key);
  }
  expect(
    await page.evaluate(
      () => (window as unknown as { taken: boolean[] }).taken,
    ),
  ).toEqual([false, false, false, false]);
  await expect(page).toHaveURL(/\/journals\/$/);
  await expect(editor).toBeFocused();
});
