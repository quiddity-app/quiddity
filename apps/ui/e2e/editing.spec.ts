import { expect, test, editBlock, editor, mainBlocks, openEntity } from "./fixtures";

const DDIA = "Designing Data-Intensive Applications";
const NOTES = "Notes on loro and other CRDTs";

test.beforeEach(async ({ page }) => {
  await openEntity(page, "Reading list");
});

test("clicking a block edits it, and typing changes its text", async ({ page }) => {
  await editBlock(page, DDIA);
  await expect(editor(page)).toBeFocused();
  await page.keyboard.press("End");
  await page.keyboard.type(" 2e");
  await expect(editor(page)).toHaveText(`${DDIA} 2e`);
  await page.keyboard.press("Escape");
  await expect(editor(page)).toHaveCount(0);
  expect(await mainBlocks(page)).toEqual([`${DDIA} 2e`, NOTES]);
});

test("Enter splits the block at the caret and continues in the new one", async ({ page }) => {
  await editBlock(page, DDIA);
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("Enter");
  await expect(editor(page)).toHaveText("ns");
  await page.keyboard.type("X");
  await page.keyboard.press("Escape");
  expect(await mainBlocks(page)).toEqual([DDIA.slice(0, -2), "Xns", NOTES]);
});

test("Shift-Enter inserts a newline instead", async ({ page }) => {
  await editBlock(page, DDIA);
  await page.keyboard.press("End");
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("more");
  await page.keyboard.press("Escape");
  expect(await mainBlocks(page)).toEqual([`${DDIA}\nmore`, NOTES]);
});

test("Backspace at the start joins a block to the one before", async ({ page }) => {
  await editBlock(page, NOTES);
  await page.keyboard.press("Home");
  await page.keyboard.press("Backspace");
  await expect(editor(page)).toHaveText(DDIA + NOTES);
  // The caret sits where they were joined.
  await page.keyboard.type("|");
  await page.keyboard.press("Escape");
  expect(await mainBlocks(page)).toEqual([`${DDIA}|${NOTES}`]);
});

test("Backspace at the very start of an entity does nothing", async ({ page }) => {
  await editBlock(page, DDIA);
  await page.keyboard.press("Home");
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Escape");
  expect(await mainBlocks(page)).toEqual([DDIA, NOTES]);
});

test("Backspace in an empty block removes it and edits the end of the one before", async ({ page }) => {
  await page.locator("main").getByRole("button", { name: "+ Add a block" }).click();
  await page.keyboard.press("Backspace");
  await expect(editor(page)).toHaveText(NOTES);
  await page.keyboard.type("!");
  await page.keyboard.press("Escape");
  expect(await mainBlocks(page)).toEqual([DDIA, `${NOTES}!`]);
});

test("Tab and Shift-Tab indent and outdent, without losing the caret", async ({ page }) => {
  await editBlock(page, NOTES);
  await page.keyboard.press("End");
  await page.keyboard.press("Tab");
  await expect(page.locator("main article > ul > li > div > div > ul")).toHaveCount(1);
  await expect(editor(page)).toBeFocused();
  await page.keyboard.type("1");
  await page.keyboard.press("Shift+Tab");
  await expect(page.locator("main article > ul > li > div > div > ul")).toHaveCount(0);
  await expect(editor(page)).toBeFocused();
  await page.keyboard.type("2");
  await page.keyboard.press("Escape");
  expect(await mainBlocks(page)).toEqual([DDIA, `${NOTES}12`]);
});

test("Enter in an empty block outdents it, unless it is top level", async ({ page }) => {
  await editBlock(page, NOTES);
  await page.keyboard.press("End");
  await page.keyboard.press("Tab");
  await page.keyboard.press("Enter");
  const nested = page.locator("main article > ul > li > div > div > ul > li");
  await expect(nested).toHaveCount(2);
  await page.keyboard.press("Enter");
  await expect(nested).toHaveCount(1);
  await expect(editor(page)).toBeFocused();
  // At the top level an empty block has nowhere to go, so Enter adds another.
  await page.keyboard.press("Enter");
  await page.keyboard.type("x");
  await page.keyboard.press("Escape");
  expect(await mainBlocks(page)).toEqual([DDIA, NOTES, "", "x"]);
});

test("Alt-Up and Alt-Down move a block among its siblings", async ({ page }) => {
  await editBlock(page, NOTES);
  await page.keyboard.press("Alt+ArrowUp");
  await expect.poll(() => mainBlocks(page)).toEqual([NOTES, DDIA]);
  await page.keyboard.press("Alt+ArrowDown");
  await expect.poll(() => mainBlocks(page)).toEqual([DDIA, NOTES]);
});

test("the arrows move between blocks at their edges", async ({ page }) => {
  await editBlock(page, DDIA);
  await page.keyboard.press("ArrowDown");
  await expect(editor(page)).toHaveText(NOTES);
  await page.keyboard.press("ArrowUp");
  await expect(editor(page)).toHaveText(DDIA);
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowRight");
  await expect(editor(page)).toHaveText(NOTES);
  await page.keyboard.press("Home");
  await page.keyboard.press("ArrowLeft");
  await expect(editor(page)).toHaveText(DDIA);
});

test("a block can be added at the end", async ({ page }) => {
  await page.locator("main").getByRole("button", { name: "+ Add a block" }).click();
  await page.keyboard.type("brand new");
  await page.keyboard.press("Escape");
  expect(await mainBlocks(page)).toEqual([DDIA, NOTES, "brand new"]);
});

test("a block open in the main pane and a side card stays in step both ways", async ({ page }) => {
  await page.goto("/all/");
  const listed = page.locator("main").getByRole("link", { name: "Reading list", exact: true });
  await listed.click({ modifiers: ["Shift"] });
  await listed.click();
  const card = page.locator("aside article");
  await expect(card.getByText(NOTES)).toBeVisible();

  await editBlock(page, NOTES);
  await page.keyboard.press("End");
  await page.keyboard.type("!!");
  await expect(card.getByText(`${NOTES}!!`)).toBeVisible();
  await page.keyboard.press("Escape");

  await card.getByText(`${NOTES}!!`).click();
  await page.keyboard.type("??");
  await expect(page.locator("main article > ul").getByText(`${NOTES}!!??`)).toBeVisible();
});

test("a new entity can be made, written in, and found again", async ({ page }) => {
  await page.goto("/all/");
  await page.getByLabel("Name of a new entity").fill("Fresh page");
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Fresh page", level: 1 })).toBeVisible();
  await page.locator("main article > ul [data-block-text]").first().click();
  await page.keyboard.type("first thought");
  await page.keyboard.press("Enter");
  await page.keyboard.type("second");
  await page.keyboard.press("Tab");
  await page.keyboard.press("Escape");
  expect(await mainBlocks(page)).toEqual(["first thought", "second"]);
  await expect(page.locator("main article > ul > li > div > div > ul")).toHaveCount(1);
});

test("today's journal takes notes in the feed", async ({ page }) => {
  await page.goto("/");
  const today = page.locator("main section").first();
  await expect(today.getByRole("heading", { level: 2 })).toBeVisible();
  await today.locator("[data-block-text]").last().click();
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("in today");
  await page.keyboard.press("Escape");
  await expect(today.locator("[data-block-text]").filter({ hasText: "in today" })).toBeVisible();
});

test("leaving a block never shows its old text", async ({ page }) => {
  await editBlock(page, DDIA);
  await page.keyboard.press("End");
  await page.keyboard.type("!");
  await page.keyboard.press("Escape");
  // Sampled straight away, before any pause could have caught it up.
  expect(await mainBlocks(page)).toEqual([`${DDIA}!`, NOTES]);
});
