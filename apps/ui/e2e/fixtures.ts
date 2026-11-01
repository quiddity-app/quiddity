import { expect, test as base, type Locator, type Page } from "@playwright/test";

/** A test that fails if the page logs an error, which the UI shouldn't. */
export const test = base.extend<{ page: Page }>({
  page: async ({ page }, use) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
    await use(page);
    expect(errors, "errors logged by the page").toEqual([]);
  },
});

/** What the memory backend lets a test see and do as the shell and as other devices. */
declare global {
  interface Window {
    __quiddityRemote?: {
      createEntity(name: string): void;
      /** The blocks the page last said it was editing. */
      focus: string[];
      /** What each push said it was typing in. */
      typed: string[][];
      /** The devices the page asked to let in. */
      allowed: string[];
      /** Plays the shell telling the page how this device stands. */
      setStatus(status: string): void;
    };
  }
}

export { expect };

/** The text of every block of the entity in the main pane, in reading order. */
export function mainBlocks(page: Page): Promise<string[]> {
  return blockTexts(page, "main article > ul");
}

/** The same for one outline, found by selector. */
export function blockTexts(page: Page, outline: string): Promise<string[]> {
  return page.evaluate(
    (outline) =>
      [...document.querySelectorAll(`${outline} li`)].map((li) => {
        const own = li.querySelector(
          ":scope > div > [data-block-text], :scope > div > div .cm-content",
        );
        return own?.textContent ?? "";
      }),
    outline,
  );
}

/** Opens an entity by name, through the list of all of them. */
export async function openEntity(page: Page, name: string) {
  await page.goto("/all/");
  await page.locator("main").getByRole("link", { name, exact: true }).first().click();
  await expect(page.getByRole("heading", { name, level: 1 })).toBeVisible();
}

/** The block editor that has the focus. */
export const editor = (page: Page): Locator => page.locator(".cm-content");

/** Clicks the block with this text in the main pane, to edit it. */
export function editBlock(page: Page, text: string) {
  return page.locator("main article > ul").getByText(text, { exact: true }).click();
}

/** Clicks somewhere that isn't a block, so nothing is being edited. */
export async function clickAway(page: Page) {
  await page.locator("body").click({ position: { x: 5, y: 5 } });
}
