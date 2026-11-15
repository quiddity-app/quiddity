import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { checkExport } from "./check-export.mjs";

function site(pages) {
  const out = mkdtempSync(join(tmpdir(), "export-"));
  for (const [name, html] of Object.entries(pages)) {
    mkdirSync(join(out, "page", name), { recursive: true });
    writeFileSync(join(out, "page", name, "index.html"), html);
  }
  return out;
}

const drawn = (name, heading = name) =>
  `<html><head><title>${name} – Quiddity</title></head><body><main><div><h1 class="x">${heading}</h1><p>Text</p></div></main></body></html>`;
const shell = (name) =>
  `<html><head><title>${name} – Quiddity</title></head><body><main><div class="animate-pulse"></div></main></body></html>`;

test("a page drawn with its heading and title passes", () => {
  assert.deepEqual(checkExport(site({ "Static build": drawn("Static build") }), ["Static build"]), []);
});

test("a heading is compared as text, so its markup and escapes don't matter", () => {
  const html = drawn("Q & A", '<button title="Rename">Q &amp; A</button>');
  assert.deepEqual(checkExport(site({ "Q & A": html }), ["Q & A"]), []);
});

test("a shell with no heading fails: that is the page before this check", () => {
  const problems = checkExport(site({ Loro: shell("Loro") }), ["Loro"]);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /Loro: the page's own heading/);
});

test("another page's heading, or title, fails", () => {
  const html = drawn("Other").replace("Other – Quiddity", "Quiddity");
  assert.equal(checkExport(site({ Loro: html }), ["Loro"]).length, 2);
});

test("a heading outside the main pane doesn't count", () => {
  const html = `<html><title>Loro – Quiddity</title><h1>Loro</h1><main></main></html>`;
  assert.equal(checkExport(site({ Loro: html }), ["Loro"]).length, 1);
});

test("a name with no page fails, and no names is not a pass", () => {
  assert.match(checkExport(site({}), ["Loro"])[0], /^Loro: no page/);
  assert.deepEqual(checkExport(site({}), []), ["there are no names to check"]);
});
