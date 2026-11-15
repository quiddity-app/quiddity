// Checks that a published site's export has each named page drawn in its HTML,
// not left as the shell the browser fills in (docs/Page names.md): without
// JavaScript, a crawler or a reader sees only what is in the file.
//
//   node scripts/check-export.mjs apps/ui/out target/docs-graph/names.txt
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const ENTITIES = { "&amp;": "&", "&lt;": "<", "&gt;": ">", "&quot;": '"', "&#x27;": "'", "&#39;": "'" };

const text = (html) =>
  html
    .replace(/<[^>]*>/g, "")
    .replace(/&(amp|lt|gt|quot|#x27|#39);/g, (entity) => ENTITIES[entity])
    .trim();

/** What is wrong with the export in `out`, one line each, for the pages `names` lists. */
export function checkExport(out, names) {
  const problems = [];
  if (names.length === 0) problems.push("there are no names to check");
  for (const name of names) {
    const file = join(out, "page", name, "index.html");
    if (!existsSync(file)) {
      problems.push(`${name}: no page at ${file}`);
      continue;
    }
    const html = readFileSync(file, "utf8");
    const heading = /<main[^>]*>[\s\S]*?<h1[^>]*>([\s\S]*?)<\/h1>/.exec(html);
    // The shell has a skeleton where the heading goes, so a heading that says
    // the name is the page itself.
    if (!heading || text(heading[1]) !== name) {
      problems.push(`${name}: the page's own heading is not in its HTML`);
    }
    const title = /<title>([\s\S]*?)<\/title>/.exec(html);
    if (!title || text(title[1]) !== `${name} – Quiddity`) {
      problems.push(`${name}: the title is not the page's name`);
    }
  }
  return problems;
}

function main() {
  const [out, namesFile] = process.argv.slice(2);
  if (!out || !namesFile) throw new Error("usage: check-export.mjs OUT NAMES");
  const names = readFileSync(namesFile, "utf8").split("\n").filter(Boolean);
  const problems = checkExport(out, names);
  for (const problem of problems) console.error(problem);
  if (problems.length) process.exit(1);
  console.log(`${names.length} pages are drawn in the export`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) main();
