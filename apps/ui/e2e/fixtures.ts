import { expect, test as base, type Locator, type Page } from "@playwright/test";

/**
 * A test that fails if the page logs an error, which the UI shouldn't.
 *
 * A test that provokes errors on purpose says which with `allowedErrors`:
 * React reports an error an error boundary has caught, a moment after the
 * fallback appears, so without it such a test passes only if it ends before
 * the report arrives, and fails on a slow machine.
 */
export const test = base.extend<{ page: Page; allowedErrors: RegExp[] }>({
  allowedErrors: [[], { option: true }],
  page: async ({ page, allowedErrors }, use) => {
    const errors: string[] = [];
    const record = (text: string) => {
      if (!allowedErrors.some((allowed) => allowed.test(text))) errors.push(text);
    };
    page.on("pageerror", (e) => record(String(e)));
    page.on("console", (m) => m.type() === "error" && record(m.text()));
    await use(page);
    expect(errors, "errors logged by the page").toEqual([]);
  },
});

/** What the memory backend lets a test see and do as the shell and as other devices. */
declare global {
  interface Window {
    __quiddityRemote?: {
      createEntity(name: string): void;
      /** A file another device has added to a page, whose bytes this one doesn't have. */
      createBlob(page: string, name: string, hash: string, mime: string, size: number): void;
      /** The hashes of the files the page has kept. */
      kept: string[];
      /** The blocks the page last said it was editing. */
      focus: string[];
      /** What each push said it was typing in. */
      typed: string[][];
      /** The devices the page asked to let in. */
      allowed: string[];
      /** How the page asked to go on from a graph that wouldn't open. */
      recovered: string[];
      /** Plays the shell telling the page how this device stands. */
      setStatus(status: string): void;
    };
  }
}

/**
 * Opens the search panel with Ctrl-K. The shortcut is only heard once the page
 * has hydrated, so a key pressed straight after `goto` can be lost on a slow
 * machine: press again until the panel is there. It never presses while the
 * panel is open, since the key would close it.
 */
export async function openSearch(page: Page): Promise<void> {
  const panel = page.getByRole("dialog", { name: "Search" });
  await expect(async () => {
    if (!(await panel.isVisible())) await page.keyboard.press("Control+k");
    await expect(panel).toBeVisible({ timeout: 1_000 });
  }).toPass({ timeout: 15_000 });
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
