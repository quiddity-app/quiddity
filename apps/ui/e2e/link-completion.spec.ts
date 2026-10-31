import { expect, test, editor, openEntity } from "./fixtures";

const block = (page: import("@playwright/test").Page) =>
  page.locator("main article > ul").getByText("Local-first");

/** Starts a new line of text at the end of the block. */
async function typeAtEnd(page: import("@playwright/test").Page, text: string) {
  await block(page).click();
  await page.keyboard.press("End");
  await page.keyboard.type(text);
}

test.beforeEach(async ({ page }) => {
  await openEntity(page, "Quiddity");
});

test("[ makes a pair, [[ a double pair, and a selection is wrapped", async ({ page }) => {
  await typeAtEnd(page, " [");
  expect(await editor(page).textContent()).toMatch(/ \[\]$/);
  await page.keyboard.type("[x");
  expect(await editor(page).textContent()).toMatch(/ \[\[x\]\]$/);
  // `]` steps over the closer rather than doubling it.
  await page.keyboard.type("]]");
  expect(await editor(page).textContent()).toMatch(/ \[\[x\]\]$/);

  await page.keyboard.type(" word");
  await page.keyboard.press("Shift+Control+ArrowLeft");
  await page.keyboard.type("[");
  expect(await editor(page).textContent()).toMatch(/ \[\[x\]\] \[word\]$/);
});

test("Backspace inside an empty pair removes both", async ({ page }) => {
  await typeAtEnd(page, " [[");
  await page.keyboard.press("Backspace");
  expect(await editor(page).textContent()).toMatch(/ \[\]$/);
  await page.keyboard.press("Backspace");
  expect(await editor(page).textContent()).not.toMatch(/\[/);
});

test("a new link offers pages with their properties; arrows put one in the link", async ({ page }) => {
  await typeAtEnd(page, " [[Lor");
  const options = page.getByRole("option");
  await expect(options.first()).toContainText("Loro");
  // The properties are shown beside the title.
  await expect(options.first()).toContainText(/\w+: \w+/);

  await page.keyboard.press("ArrowDown");
  await expect(options.first()).toHaveAttribute("aria-selected", "true");
  expect(await editor(page).textContent()).toMatch(/ \[\[Loro\]\]$/);
  // Back past the top is what was typed.
  await page.keyboard.press("ArrowUp");
  expect(await editor(page).textContent()).toMatch(/ \[\[Lor\]\]$/);

  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(options).toHaveCount(0);
  // Accepting steps out of the link, so typing carries on after it.
  await page.keyboard.type(" ok");
  expect(await editor(page).textContent()).toMatch(/ \[\[Loro\]\] ok$/);
});

test("Enter with nothing chosen accepts the text as written", async ({ page }) => {
  await typeAtEnd(page, " [[Zzz Nowhere");
  await page.keyboard.press("Enter");
  await page.keyboard.type("!");
  expect(await editor(page).textContent()).toMatch(/ \[\[Zzz Nowhere\]\]!$/);
});

test("clicking an entry puts it in the link", async ({ page }) => {
  await typeAtEnd(page, " [[Lor");
  await page.getByRole("option").first().click();
  expect(await editor(page).textContent()).toMatch(/ \[\[Loro\]\]$/);
});

test("same-titled pages are completed with the qualifiers that tell them apart", async ({ page }) => {
  await typeAtEnd(page, " [[Mercury#type(planet)]] [[Mercury#type(element)]]");
  await page.keyboard.press("Escape");
  // The block is longer now, and its middle is a link.
  await block(page).click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("End");
  await page.keyboard.type(" [[Merc");
  const options = page.getByRole("option");
  await expect(options).toHaveCount(2);
  await expect(options.filter({ hasText: "#type(element)" })).toHaveCount(1);
  await options.filter({ hasText: "#type(planet)" }).click();
  expect(await editor(page).textContent()).toMatch(/ \[\[Mercury#type\(planet\)\]\]$/);
});

test("Enter in a link that wasn't being completed follows it", async ({ page }) => {
  await page.locator("main article > ul").getByText("Built on", { exact: false }).click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("Home");
  // Into "Lo|ro": the caret has arrived in the link, nothing was typed.
  for (let i = 0; i < "Built on [[Lo".length; i++) await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("option")).toHaveCount(0);
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Loro", level: 1 })).toBeVisible();
});
