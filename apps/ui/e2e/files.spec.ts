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

test("a dropped file becomes an entity, linked from the page, with its bytes kept", async ({ page }) => {
  await openEntity(page, "Reading list");
  await heading(page).dispatchEvent("drop", { dataTransfer: await transfer(page, [text("notes.txt", "hello")]) });

  const link = page.locator("main article").getByRole("link", { name: "notes.txt" });
  await expect(link).toBeVisible();
  expect(await page.evaluate(() => window.__quiddityRemote?.kept.length)).toBe(1);

  await link.click();
  await expect(page.getByRole("heading", { level: 1, name: "notes.txt" })).toBeVisible();
  const download = page.getByRole("link", { name: "Download notes.txt" });
  await expect(download).toHaveAttribute("href", /^blob:/);
  await expect(download).toHaveAttribute("download", "notes.txt");
  await expect(page.getByText("5 bytes, text/plain")).toBeVisible();
});

test("an image is shown, from its own bytes", async ({ page }) => {
  await openEntity(page, "Reading list");
  await heading(page).dispatchEvent("drop", {
    dataTransfer: await transfer(page, [{ name: "dot.png", type: "image/png", data: dot }]),
  });
  await page.locator("main article").getByRole("link", { name: "dot.png" }).click();

  const image = page.getByRole("img", { name: "dot.png" });
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
  await expect(page.locator("main article").getByRole("link", { name: "pasted.txt" })).toBeVisible();
});

test("several files at once each get an entity", async ({ page }) => {
  await openEntity(page, "Reading list");
  await heading(page).dispatchEvent("drop", {
    dataTransfer: await transfer(page, [text("one.txt", "1"), text("two.txt", "22")]),
  });
  const article = page.locator("main article");
  await expect(article.getByRole("link", { name: "one.txt" })).toBeVisible();
  await expect(article.getByRole("link", { name: "two.txt" })).toBeVisible();
  expect(await page.evaluate(() => window.__quiddityRemote?.kept.length)).toBe(2);
});

test("adding a file is one undo step", async ({ page }) => {
  await openEntity(page, "Reading list");
  await heading(page).dispatchEvent("drop", { dataTransfer: await transfer(page, [text("undo.txt", "u")]) });
  const link = page.locator("main article").getByRole("link", { name: "undo.txt" });
  await expect(link).toBeVisible();

  await page.keyboard.press("ControlOrMeta+z");
  await expect(link).toHaveCount(0);
  // Its entity went with the link.
  await openAll(page);
  await expect(page.locator("main").getByRole("link", { name: "undo.txt" })).toHaveCount(0);
});

test("a file another device has says it isn't here yet, and arrives when asked for again", async ({ page }) => {
  const bytes = text("far.txt", "far away");
  await openEntity(page, "Reading list");
  // Another device describes a file this one has no bytes for.
  await page.evaluate(async (data) => {
    const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
    const digest = await crypto.subtle.digest("SHA-256", bytes);
    const hash = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
    window.__quiddityRemote!.createAsset("far.txt", hash, "text/plain", bytes.length);
  }, bytes.data);

  await openAll(page);
  await page.locator("main").getByRole("link", { name: "far.txt", exact: true }).click();
  await expect(page.getByRole("heading", { name: "far.txt", level: 1 })).toBeVisible();
  await expect(page.getByText("isn’t on this device yet")).toBeVisible();
  await expect(page.getByRole("link", { name: "Download far.txt" })).toHaveCount(0);

  // Asking again with nothing new changes nothing.
  await page.getByRole("button", { name: "Try again" }).click();
  await expect(page.getByText("isn’t on this device yet")).toBeVisible();

  // The same bytes turn up here: dropped on this page, as any file is.
  await heading(page).dispatchEvent("drop", { dataTransfer: await transfer(page, [bytes]) });
  await page.getByRole("button", { name: "Try again" }).click();
  await expect(page.getByRole("link", { name: "Download far.txt" }).first()).toBeVisible();
});
