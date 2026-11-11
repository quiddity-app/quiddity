import { expect, openEntity, test } from "./fixtures";
import type { Page } from "@playwright/test";

/** A one pixel PNG. */
const dot =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

interface Upload {
  name: string;
  type: string;
  /** Base64, so that binary survives the trip into the page. */
  data: string;
}

const text = (name: string, contents: string): Upload => ({
  name,
  type: "text/plain",
  data: Buffer.from(contents).toString("base64"),
});

/** What a drop or a paste carries: files, as a person would bring them. */
function transfer(page: Page, files: Upload[]) {
  return page.evaluateHandle((files) => {
    const data = new DataTransfer();
    for (const f of files) {
      const bytes = Uint8Array.from(atob(f.data), (c) => c.charCodeAt(0));
      data.items.add(new File([bytes], f.name, { type: f.type }));
    }
    return data;
  }, files);
}

/** Moves to the list of all entities by clicking, since a reload would start
 * the made-up graph over. */
async function openAll(page: Page) {
  await page.locator("nav").getByRole("link", { name: "All entities" }).click();
  await expect(page.getByRole("heading", { name: "All entities", level: 1 })).toBeVisible();
}

const heading = (page: Page) => page.locator("main article h1");

const article = (page: Page) => page.locator("main article");

/** The id of the entity open in the main pane, from the address. */
const openId = (page: Page) => new URL(page.url()).searchParams.get("id")!;

test("a dropped file is shown inline in the page, with its bytes kept, and makes no page", async ({ page }) => {
  await openEntity(page, "Reading list");
  await heading(page).dispatchEvent("drop", { dataTransfer: await transfer(page, [text("notes.txt", "hello")]) });

  const download = article(page).getByRole("link", { name: "Download notes.txt" });
  await expect(download).toHaveAttribute("href", /^blob:/);
  await expect(download).toHaveAttribute("download", "notes.txt");
  await expect(article(page).getByText("5 bytes, text/plain")).toBeVisible();
  expect(await page.evaluate(() => window.__quiddityRemote?.kept.length)).toBe(1);

  // It is a block of the page, not an entity of its own.
  await openAll(page);
  await expect(page.locator("main").getByRole("link", { name: "notes.txt" })).toHaveCount(0);
});

test("an image is shown inline, from its own bytes", async ({ page }) => {
  await openEntity(page, "Reading list");
  await heading(page).dispatchEvent("drop", {
    dataTransfer: await transfer(page, [{ name: "dot.png", type: "image/png", data: dot }]),
  });

  const image = article(page).getByRole("img", { name: "dot.png" });
  await expect(image).toBeVisible();
  await expect(image).toHaveJSProperty("naturalWidth", 1);
});

test("pasting a file works like dropping one", async ({ page }) => {
  await openEntity(page, "Reading list");
  // Playwright's own dispatchEvent doesn't make a ClipboardEvent, so make one.
  const clipboardData = await transfer(page, [text("pasted.txt", "x")]);
  await heading(page).evaluate(
    (el, clipboardData) =>
      el.dispatchEvent(new ClipboardEvent("paste", { clipboardData, bubbles: true, cancelable: true })),
    clipboardData,
  );
  await expect(article(page).getByRole("link", { name: "Download pasted.txt" })).toBeVisible();
});

test("several files at once each get a block", async ({ page }) => {
  await openEntity(page, "Reading list");
  await heading(page).dispatchEvent("drop", {
    dataTransfer: await transfer(page, [text("one.txt", "1"), text("two.txt", "22")]),
  });
  await expect(article(page).getByRole("link", { name: "Download one.txt" })).toBeVisible();
  await expect(article(page).getByRole("link", { name: "Download two.txt" })).toBeVisible();
  expect(await page.evaluate(() => window.__quiddityRemote?.kept.length)).toBe(2);
});

test("adding a file is one undo step", async ({ page }) => {
  await openEntity(page, "Reading list");
  await heading(page).dispatchEvent("drop", { dataTransfer: await transfer(page, [text("undo.txt", "u")]) });
  const download = article(page).getByRole("link", { name: "Download undo.txt" });
  await expect(download).toBeVisible();

  await page.keyboard.press("ControlOrMeta+z");
  await expect(download).toHaveCount(0);
});

test("text after a file's line is its caption", async ({ page }) => {
  await openEntity(page, "Reading list");
  await heading(page).dispatchEvent("drop", { dataTransfer: await transfer(page, [text("cap.txt", "c")]) });
  const download = article(page).getByRole("link", { name: "Download cap.txt" });
  await expect(download).toBeVisible();

  // Editing shows the line, and what is typed after it is the caption.
  // `has` is relative to the block, so it can't be the page-rooted `download`.
  const block = article(page)
    .locator("[data-block-text]")
    .filter({ has: page.getByRole("link", { name: "Download cap.txt" }) });
  // By keyboard: a click anywhere near the top would land on the download link,
  // which does its own thing and doesn't start editing.
  await block.focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".cm-content")).toContainText("##blob(");
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("a note on the file");
  await page.keyboard.press("Escape");

  await expect(article(page).getByText("a note on the file")).toBeVisible();
  await expect(download).toBeVisible();
  await expect(article(page).getByText("##blob(")).toHaveCount(0);
});

test("a file another device has says it isn't here yet, and arrives when asked for again", async ({ page }) => {
  const bytes = text("far.txt", "far away");
  await openEntity(page, "Reading list");
  // Another device adds a file to this page whose bytes this one doesn't have.
  await page.evaluate(
    async ({ data, id }) => {
      const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
      const digest = await crypto.subtle.digest("SHA-256", bytes);
      const hash = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
      window.__quiddityRemote!.createBlob(id, "far.txt", hash, "text/plain", bytes.length);
    },
    { data: bytes.data, id: openId(page) },
  );

  await expect(article(page).getByText("isn’t on this device yet")).toBeVisible();
  await expect(article(page).getByRole("link", { name: "Download far.txt" })).toHaveCount(0);

  // Asking again with nothing new changes nothing.
  await article(page).getByRole("button", { name: "Try again" }).click();
  await expect(article(page).getByText("isn’t on this device yet")).toBeVisible();

  // The same bytes turn up here: dropped on this page, as any file is.
  await heading(page).dispatchEvent("drop", { dataTransfer: await transfer(page, [bytes]) });
  await article(page).getByRole("button", { name: "Try again" }).click();
  await expect(article(page).getByRole("link", { name: "Download far.txt" }).first()).toBeVisible();
});
