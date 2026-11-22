import { clickAway, editBlock, expect, openEntity, test } from './fixtures';

import type { Page } from '@playwright/test';

// The page graph is the right pane's graph of the main entity: drawn on a
// canvas like the graph view, with a real link for each entity beside it.
// The one being looked at is the link with `aria-current`.

const graph = (page: Page) =>
  page.getByRole('group', { name: 'Entities near this page' });
const nodes = async (page: Page) =>
  (await graph(page).getByRole('link').allTextContents()).sort();

async function show(page: Page) {
  await page.getByRole('button', { name: 'More' }).click();
  await page.getByRole('menuitem', { name: 'Show Page Graph' }).click();
  await expect(graph(page).locator('canvas')).toBeVisible();
}

test('the page graph needs an entity to be about', async ({ page }) => {
  await page.goto('/journals/');
  await page.getByRole('button', { name: 'More' }).click();
  await expect(
    page.getByRole('menuitem', { name: 'Show Page Graph' }),
  ).toBeDisabled();
});

test('the page graph shows the page and its neighbours, and picks out the page', async ({
  page,
}) => {
  await openEntity(page, 'Quiddity');
  await show(page);
  // Loro and iroh are linked from it, and so is `project`, which its
  // `type:: project` names: nobody has made that page, but the link is there.
  // CRDT is only a step beyond Loro, with nothing else tying it in, and
  // journals that mention it are left out.
  await expect
    .poll(() => nodes(page))
    .toEqual(['Loro', 'Quiddity', 'iroh', 'project']);
  await expect(graph(page).locator('[aria-current="page"]')).toHaveText(
    'Quiddity',
  );

  await page.getByRole('button', { name: 'More' }).click();
  await expect(
    page.getByRole('menuitem', { name: 'Hide Page Graph' }),
  ).toBeVisible();
  await page.getByRole('menuitem', { name: 'Hide Page Graph' }).click();
  await expect(graph(page)).toHaveCount(0);
});

test('a node of the page graph opens its entity, keeping the graph', async ({
  page,
}) => {
  await openEntity(page, 'Quiddity');
  await show(page);
  // The links are for keyboards, and have no size to click.
  await graph(page).getByRole('link', { name: 'Loro', exact: true }).focus();
  await page.keyboard.press('Enter');
  await expect(
    page.getByRole('heading', { name: 'Loro', level: 1 }),
  ).toBeVisible();
  await expect(graph(page).locator('[aria-current="page"]')).toHaveText('Loro');
});

test('two steps away is in only if it touches two neighbours', async ({
  page,
}) => {
  // CRDT is a step beyond Loro, but only a property of it, which is no link
  // here. Once Loro and iroh both link to it, it ties two neighbours together.
  // A reload would start the graph over, so it moves by links.
  const sidebar = page
    .locator('nav')
    .getByRole('link', { name: 'Quiddity', exact: true })
    .first();
  const linkCrdt = async (text: string) => {
    await editBlock(page, text);
    await page.keyboard.press('End');
    await page.keyboard.type(' [[CRDT]]');
    await page.keyboard.press('Escape');
    await clickAway(page);
  };
  await openEntity(page, 'Loro');
  await linkCrdt('A CRDT library. Documents are movable trees of text.');
  await sidebar.click();
  await page
    .locator('main')
    .getByRole('link', { name: 'iroh', exact: true })
    .first()
    .click();
  await expect(
    page.getByRole('heading', { name: 'iroh', level: 1 }),
  ).toBeVisible();
  await linkCrdt('QUIC with hole punching, addressed by public key.');
  await sidebar.click();
  await expect(
    page.getByRole('heading', { name: 'Quiddity', level: 1 }),
  ).toBeVisible();
  await show(page);
  await expect
    .poll(() => nodes(page))
    .toEqual(['CRDT', 'Loro', 'Quiddity', 'iroh', 'project']);
  await expect(graph(page).locator('[aria-current="page"]')).toHaveText(
    'Quiddity',
  );
});
