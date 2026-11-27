#!/usr/bin/env node
// Makes `licenses.json`, the list of everything Quiddity is built from and the
// licence each part is under, which the app shows on its About page. See
// docs/Licences.md.
//
// Most of the licences Quiddity depends on (MIT, BSD, Apache-2.0) are only
// granted on condition that their text travels with the software, so this is
// compliance as well as courtesy. It collects:
//
// - the Rust crates that end up in the app and in the WASM the UI embeds,
//   found with `cargo metadata` by following normal dependencies from
//   `quiddity-app` and `quiddity-wasm` (build and dev dependencies are not
//   shipped), and
// - the npm packages the UI depends on in production, from `pnpm licenses`.
//
// Both lists are a little wider than what ships (every target's crates, not just
// the one being built, and proc macros), which errs towards saying too much.
//
// A package under a licence outside the allow list below is an error, as is
// one with no licence text to ship: someone should look at it, and then either
// change the list or the dependency.
//
//     node scripts/licenses.mjs
//
// It takes no arguments: it always writes `apps/ui/public/licenses.json`, from
// wherever it is run, so there is no path for anyone to steer it by.

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

/** The crates whose dependencies are shipped. */
export const ROOTS = ["quiddity-app", "quiddity-wasm"];

/** Licences Quiddity is happy to ship, by SPDX identifier. */
export const ALLOWED = new Set([
  "0BSD",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "BSL-1.0",
  "CC0-1.0",
  "CDLA-Permissive-2.0",
  "ISC",
  // Fonts and the browser-support data, both of which ask only for their text to travel.
  "OFL-1.1",
  "CC-BY-4.0",
  // File-level copyleft: fine while the files themselves aren't modified.
  "MPL-2.0",
  "MIT",
  "Unicode-3.0",
  "Unicode-DFS-2016",
  "Unlicense",
  "Zlib",
]);

/**
 * npm packages that are installed but never shipped, with why. Next's image
 * optimiser pulls in sharp and its LGPL libvips, but the UI is a static export
 * with `images.unoptimized`, so neither is in the output.
 */
export const NOT_SHIPPED = [
  { prefix: "@img/", reason: "sharp's binaries serve next/image, which a static export doesn't use" },
];

/**
 * Whether an SPDX expression can be satisfied with only allowed licences.
 * Cargo's older `A/B` means `A OR B`; `X WITH exception` is `X`, since
 * exceptions only ever widen what a licence allows.
 */
export function permitted(expression, allowed = ALLOWED) {
  const tokens = expression
    .replaceAll("/", " OR ")
    .replaceAll("(", " ( ")
    .replaceAll(")", " ) ")
    .split(/\s+/)
    .filter(Boolean);
  let at = 0;
  const peek = () => tokens[at];
  const or = () => {
    let ok = and();
    while (peek() === "OR") {
      at++;
      // `and` first, so that both sides are always consumed.
      const right = and();
      ok = ok || right;
    }
    return ok;
  };
  const and = () => {
    let ok = atom();
    while (peek() === "AND") {
      at++;
      const right = atom();
      ok = ok && right;
    }
    return ok;
  };
  const atom = () => {
    const token = tokens[at++];
    if (token === undefined) throw new Error(`incomplete licence expression: ${expression}`);
    if (token === "(") {
      const ok = or();
      if (tokens[at++] !== ")") throw new Error(`unbalanced licence expression: ${expression}`);
      return ok;
    }
    if (["AND", "OR", "WITH", ")"].includes(token))
      throw new Error(`unexpected ${token} in licence expression: ${expression}`);
    // `-or-later` and `+` only widen the licence to later versions of it.
    const id = token.replace(/\+$/, "");
    if (peek() === "WITH") at += 2;
    return allowed.has(id);
  };
  const ok = or();
  if (at !== tokens.length) throw new Error(`unexpected ${tokens[at]} in licence expression: ${expression}`);
  return ok;
}

/** The names of the files in a package's directory that hold licence text. */
export function licenceFiles(names) {
  return names.filter((name) => /^(licen[cs]e|copying|unlicen[cs]e|notice)([-_.].*)?$/i.test(name)).sort();
}

/**
 * The crates reachable from `roots` by normal dependencies, as `cargo metadata`
 * describes them, leaving out the workspace's own.
 */
export function shippedCrates(metadata, roots = ROOTS) {
  const byId = new Map(metadata.packages.map((p) => [p.id, p]));
  const nodes = new Map(metadata.resolve.nodes.map((n) => [n.id, n]));
  const members = new Set(metadata.workspace_members);
  const seen = new Set();
  const queue = roots.map((name) => {
    const root = metadata.packages.find((p) => p.name === name && members.has(p.id));
    if (!root) throw new Error(`no workspace crate called ${name}`);
    return root.id;
  });
  while (queue.length > 0) {
    const id = queue.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      // A null kind is a normal dependency; the others are "build" and "dev".
      if (dep.dep_kinds.some((k) => k.kind === null)) queue.push(dep.pkg);
    }
  }
  return [...seen].filter((id) => !members.has(id)).map((id) => byId.get(id));
}

/**
 * `licenses.json`'s packages for crates. `read(dir)` gives the texts of the
 * licence files in a directory and `templates` the standard texts to fall back on.
 */
export function crateEntries(crates, read, templates) {
  return crates.map((crate) => {
    const license = normalise(crate.license ?? "");
    const own = read(dirname(crate.manifest_path));
    return {
      ecosystem: "rust",
      name: crate.name,
      version: crate.version,
      license,
      url: crate.repository ?? crate.homepage ?? undefined,
      texts: readOr(own, { name: crate.name, authors: crate.authors, license }, templates),
    };
  });
}

/** `licenses.json`'s packages for what `pnpm licenses list --json` says, one per version. */
export function npmEntries(listing, read, templates) {
  return Object.values(listing)
    .flat()
    .filter((pkg) => !NOT_SHIPPED.some(({ prefix }) => pkg.name.startsWith(prefix)))
    .flatMap((pkg) =>
      pkg.versions.map((version, i) => ({
        ecosystem: "npm",
        name: pkg.name,
        version,
        license: normalise(pkg.license),
        url: pkg.homepage ?? undefined,
        texts: readOr(
          read(pkg.paths[i] ?? pkg.paths[0]),
          { name: pkg.name, authors: [pkg.author ?? ""], license: normalise(pkg.license) },
          templates,
        ),
      })),
    );
}

// What a package ships, or the standard text of its licence where it ships none.
const readOr = (own, about, templates) => (own.length > 0 ? own : standardTexts(about, templates));

// Cargo's `A/B` and the spaces around parentheses, in one spelling.
const normalise = (expression) => expression.replaceAll("/", " OR ").replace(/\s+/g, " ").trim();

/**
 * The document the app reads: each distinct text once, and for each package
 * the numbers of the texts that are its own. Sorted, so that it only changes
 * when the dependencies do. Throws, listing every problem, if a package's
 * licence isn't allowed or it has no text.
 */
export function document(entries) {
  const problems = [];
  const texts = [];
  const index = new Map();
  const packages = entries
    .map((entry) => {
      if (entry.license === "" || !permitted(entry.license))
        problems.push(`${entry.name} ${entry.version}: licence ${entry.license || "(none)"} isn't allowed`);
      if (entry.texts.length === 0) problems.push(`${entry.name} ${entry.version}: no licence text`);
      const own = [
        ...new Set(
          entry.texts.map((text) => {
            if (!index.has(text)) index.set(text, texts.push(text) - 1);
            return index.get(text);
          }),
        ),
      ];
      return { ...entry, texts: own };
    })
    .sort(
      (a, b) =>
        a.name.localeCompare(b.name) ||
        a.version.localeCompare(b.version) ||
        a.ecosystem.localeCompare(b.ecosystem),
    );
  if (problems.length > 0) throw new Error(`${problems.length} problems:\n${problems.join("\n")}`);
  // Renumber texts in the order the sorted packages first use them.
  const order = [];
  const renumber = new Map();
  for (const pkg of packages) {
    pkg.texts = pkg.texts.map((n) => {
      if (!renumber.has(n)) renumber.set(n, order.push(texts[n]) - 1);
      return renumber.get(n);
    });
  }
  return { packages, texts: order };
}

/**
 * What to ship for a crate that published no licence file (the file is often
 * at the root of a workspace, outside the package): the standard text of the
 * licence, naming the authors Cargo.toml gives, or the crate's authors
 * generally where it gives none. For `A OR B` the first licence with a
 * template is enough, as the crate allows either; for `A AND B` every one is
 * needed. `templates` maps a licence's identifier to its text, where
 * `{holder}` stands for the copyright holder.
 */
export function standardTexts({ name, authors, license }, templates) {
  const ids = license.match(/[A-Za-z0-9.+-]+/g)?.filter((id) => !["AND", "OR", "WITH"].includes(id)) ?? [];
  const known = ids.filter((id) => id in templates);
  const chosen = / AND /.test(license) ? known : known.slice(0, 1);
  const holder =
    authors
      // The name is what comes before an email, however the rest is written.
      .map((author) => author.split("<")[0].trim())
      .filter(Boolean)
      .join(", ") || `The ${name} authors`;
  return chosen.map((id) => templates[id].replaceAll("{holder}", holder).trim());
}

/** The licence texts in a directory, as the files in it that look like them. */
function readTexts(dir) {
  return licenceFiles(readdirSync(dir).filter((name) => statSync(join(dir, name)).isFile())).map((name) =>
    readFileSync(join(dir, name), "utf8").trim(),
  );
}

/** The standard texts shipped beside this script, by licence identifier. */
function readTemplates() {
  const dir = join(dirname(fileURLToPath(import.meta.url)), "licence-templates");
  return Object.fromEntries(
    readdirSync(dir).map((name) => [name.replace(/\.txt$/, ""), readFileSync(join(dir, name), "utf8")]),
  );
}

function main() {
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const out = join(root, "apps/ui/public/licenses.json");
  const run = (cmd, args, cwd) =>
    JSON.parse(execFileSync(cmd, args, { cwd, encoding: "utf8", maxBuffer: 1 << 28 }));
  const metadata = run("cargo", ["metadata", "--locked", "--format-version", "1"], root);
  const listing = run("pnpm", ["licenses", "list", "--json", "--prod"], join(root, "apps/ui"));
  const templates = readTemplates();
  const result = document([
    ...crateEntries(shippedCrates(metadata), readTexts, templates),
    ...npmEntries(listing, readTexts, templates),
  ]);
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, `${JSON.stringify(result)}\n`);
  console.log(`${result.packages.length} packages, ${result.texts.length} distinct licence texts → ${out}`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) main();
