import { expect, test } from './fixtures';

// The type and the page widths: one size and spacing for a page's title, one
// reading width for text, and the monospace face that is bundled with the app.

// Every page is drawn in `Page`: its text scales with the pane as an entity's
// does, a title is one and a half times that text and a section's heading one
// and an eighth times, with the same space under a title everywhere.
for (const [path, name, sections] of [
  ['/all/', 'All entities', false],
  ['/devices/', 'Devices', true],
  ['/journals/', 'Journals', true],
  ['/stats/', 'Stats and debug', true],
] as const) {
  test(`${name} has a title scaled with its text, with the common spacing`, async ({
    page,
  }) => {
    await page.goto(path);
    const article = page.locator('main article').first();
    const size = async (el: typeof article) =>
      Number.parseFloat(await el.evaluate((e) => getComputedStyle(e).fontSize));
    const body = await size(article);
    const title = page.getByRole('heading', { name, level: 1 });
    expect(await size(title)).toBeCloseTo(body * 1.5, 1);
    await expect(title).toHaveCSS('margin-bottom', '16px');
    if (sections) {
      const section = article.getByRole('heading', { level: 2 }).first();
      expect(await size(section)).toBeCloseTo(body * 1.125, 1);
    }
  });
}

test('every page has the text size of an entity', async ({ page }) => {
  const sizes = new Set<string>();
  for (const path of ['/journals/', '/all/', '/devices/', '/stats/']) {
    await page.goto(path);
    sizes.add(
      await page
        .locator('main article')
        .first()
        .evaluate((e) => getComputedStyle(e).fontSize),
    );
  }
  await page.goto('/journals/');
  await page.locator('main article a').first().click();
  await page.waitForURL(/entity/);
  sizes.add(
    await page
      .locator('main article')
      .first()
      .evaluate((e) => getComputedStyle(e).fontSize),
  );
  expect([...sizes]).toHaveLength(1);
});

test('a reading page is the reading width, and the table page the wide one', async ({
  page,
}) => {
  await page.goto('/journals/');
  await expect(page.locator('main article').first()).toHaveCSS(
    'max-width',
    '800px',
  );
  await page.goto('/all/');
  await expect(page.locator('main article').first()).toHaveCSS(
    'max-width',
    '1152px',
  );
});

test('monospace text is set in the bundled face, which has no ligatures', async ({
  page,
}) => {
  await page.goto('/devices/');
  const loaded = await page.evaluate(async () => {
    const faces = await document.fonts.load('1em "JetBrains Mono NL"');
    return faces.map((face) => face.status);
  });
  expect(loaded).toEqual(['loaded']);
  const family = await page.evaluate(() => {
    const probe = document.createElement('code');
    probe.className = 'font-mono';
    document.body.append(probe);
    return getComputedStyle(probe).fontFamily;
  });
  expect(family.startsWith('"JetBrains Mono NL"')).toBe(true);
});
