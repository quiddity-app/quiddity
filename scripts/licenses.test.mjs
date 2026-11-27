import assert from "node:assert/strict";
import { test } from "node:test";
import {
  crateEntries,
  document,
  licenceFiles,
  npmEntries,
  permitted,
  shippedCrates,
  standardTexts,
} from "./licenses.mjs";

test("an expression is permitted when some way of reading it is", () => {
  assert.equal(permitted("MIT"), true);
  assert.equal(permitted("MIT/Apache-2.0"), true);
  assert.equal(permitted("GPL-3.0-only OR MIT"), true);
  assert.equal(permitted("GPL-3.0-only AND MIT"), false);
  assert.equal(permitted("(MIT OR GPL-3.0-only) AND Apache-2.0"), true);
  assert.equal(permitted("(GPL-3.0-only OR AGPL-3.0-only) AND Apache-2.0"), false);
  assert.equal(permitted("Apache-2.0 WITH LLVM-exception"), true);
  assert.equal(permitted("GPL-3.0-only WITH Classpath-exception-2.0"), false);
  assert.equal(permitted("MIT+"), true);
});

test("an expression that isn't one is an error rather than a guess", () => {
  for (const bad of ["", "MIT OR", "(MIT", "MIT)", "AND MIT", "MIT MIT"])
    assert.throws(() => permitted(bad), undefined, bad);
});

test("licence files are found by name", () => {
  assert.deepEqual(
    licenceFiles([
      "Cargo.toml",
      "LICENSE-MIT",
      "LICENSE-APACHE",
      "license.md",
      "NOTICE",
      "COPYING",
      "licensed.rs",
      "src",
    ]),
    ["COPYING", "LICENSE-APACHE", "LICENSE-MIT", "NOTICE", "license.md"],
  );
});

const pkg = (id, name, deps = []) => ({
  id,
  name,
  manifest_path: `/src/${name}/Cargo.toml`,
  version: "1.0.0",
  license: "MIT",
});
const node = (id, deps) => ({ id, deps: deps.map(([pkg, kind]) => ({ pkg, dep_kinds: [{ kind }] })) });

test("only what the shipped crates depend on normally is shipped", () => {
  const metadata = {
    workspace_members: ["app", "wasm", "tool"],
    packages: ["app", "wasm", "tool", "a", "b", "build", "dev", "other"].map((n) => pkg(n, n)),
    resolve: {
      nodes: [
        node("app", [
          ["a", null],
          ["build", "build"],
          ["dev", "dev"],
          ["wasm", null],
        ]),
        node("wasm", [["b", null]]),
        node("a", [["b", null]]),
        node("tool", [["other", null]]),
        node("b", []),
      ],
    },
  };
  assert.deepEqual(
    shippedCrates(metadata, ["app", "wasm"])
      .map((p) => p.name)
      .sort(),
    ["a", "b"],
  );
  assert.throws(() => shippedCrates(metadata, ["missing"]), /missing/);
});

const templates = { MIT: "(c) {holder}\nMIT text", "Apache-2.0": "Apache text", Zlib: "zlib {holder}" };

test("a package without a licence file gets its licence's standard text", () => {
  const about = (license, authors = ["Ann <ann@example.test>", "Bo"]) => ({
    name: "thing",
    authors,
    license,
  });
  assert.deepEqual(standardTexts(about("MIT"), templates), ["(c) Ann, Bo\nMIT text"]);
  assert.deepEqual(standardTexts(about("Zlib OR Apache-2.0 OR MIT"), templates), ["zlib Ann, Bo"]);
  assert.deepEqual(standardTexts(about("MIT AND Apache-2.0"), templates), [
    "(c) Ann, Bo\nMIT text",
    "Apache text",
  ]);
  assert.deepEqual(
    standardTexts(about("MIT", ["Cy <cy@example.test> (https://x.test)", "<a@b.test>"]), templates),
    ["(c) Cy\nMIT text"],
  );
  assert.deepEqual(standardTexts(about("MIT", []), templates), ["(c) The thing authors\nMIT text"]);
  assert.deepEqual(standardTexts(about("GPL-3.0-only"), templates), []);
});

test("crates and npm packages use their own text when they have it", () => {
  const crates = crateEntries(
    [{ ...pkg("x", "x"), license: "MIT/Apache-2.0", authors: [], repository: "https://example.test/x" }],
    () => ["own text"],
    templates,
  );
  assert.deepEqual(crates, [
    {
      ecosystem: "rust",
      name: "x",
      version: "1.0.0",
      license: "MIT OR Apache-2.0",
      url: "https://example.test/x",
      texts: ["own text"],
    },
  ]);
  const listing = {
    MIT: [{ name: "y", versions: ["1", "2"], paths: ["/y1", "/y2"], license: "MIT", author: "Cy" }],
    "LGPL-3.0-or-later": [
      { name: "@img/sharp-libvips", versions: ["1"], paths: ["/s"], license: "LGPL-3.0-or-later" },
    ],
  };
  const npm = npmEntries(listing, (dir) => (dir === "/y1" ? [] : [`text of ${dir}`]), templates);
  assert.deepEqual(
    npm.map((e) => [e.name, e.version, e.texts]),
    [
      ["y", "1", ["(c) Cy\nMIT text"]],
      ["y", "2", ["text of /y2"]],
    ],
  );
});

const entry = (name, license, texts, version = "1.0.0") => ({
  ecosystem: "rust",
  name,
  version,
  license,
  texts,
});

test("the document shares texts and is sorted", () => {
  const doc = document([
    entry("b", "MIT", ["m", "x"]),
    entry("a", "MIT", ["m"]),
    entry("a", "MIT", ["m"], "0.9.0"),
  ]);
  assert.deepEqual(
    doc.packages.map((p) => [p.name, p.version, p.texts]),
    [
      ["a", "0.9.0", [0]],
      ["a", "1.0.0", [0]],
      ["b", "1.0.0", [0, 1]],
    ],
  );
  assert.deepEqual(doc.texts, ["m", "x"]);
});

test("every problem is reported, not just the first", () => {
  assert.throws(
    () => document([entry("a", "GPL-3.0-only", ["t"]), entry("b", "MIT", []), entry("c", "", ["t"])]),
    (error) =>
      /3 problems/.test(error.message) &&
      /a 1.0.0: licence GPL/.test(error.message) &&
      /b 1.0.0: no licence text/.test(error.message) &&
      /c 1.0.0: licence \(none\)/.test(error.message),
  );
});
