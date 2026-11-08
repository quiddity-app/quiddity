#!/usr/bin/env node
// Which fuzz targets does a change affect? Prints them as a JSON array, for
// CI's matrix.
//
//   node scripts/fuzz-targets.mjs [<git diff range>]
//
// A target is affected by a change to its own source or seeds, to any crate it
// reaches (found from the `use` lines in the target and the dependencies in
// fuzz/Cargo.lock, so nothing is listed by hand), or to what every target
// shares: the fuzz crate's manifest and lockfile, the workspace manifest, and
// the toolchain and CI that build and run them. With no range, or one git
// can't resolve (a forced push, a new branch), every target is affected.
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** Changes to these files affect every target. */
const SHARED = [
  /^fuzz\/Cargo\.(toml|lock)$/,
  /^Cargo\.toml$/,
  /^\.cargo\//,
  /^mise\.toml$/,
  /^\.github\/workflows\/ci\.yml$/,
  /^\.github\/actions\//,
  /^rust-toolchain(\.toml)?$/,
];

/** Package name to the packages it depends on, from a Cargo.lock. */
export function lockedDependencies(lock) {
  const graph = new Map();
  for (const block of lock.split(/^\[\[package\]\]$/m).slice(1)) {
    const name = /^name = "([^"]+)"$/m.exec(block)?.[1];
    if (!name) continue;
    const list = /^dependencies = \[([^\]]*)\]/m.exec(block)?.[1] ?? "";
    // An entry is "name" or, when the lock holds two versions, "name version".
    const deps = [...list.matchAll(/"([^" ]+)(?: [^"]*)?"/g)].map((m) => m[1]);
    graph.set(name, deps);
  }
  return graph;
}

/** The workspace crates a target's source names, and all they depend on. */
export function reachableCrates(source, graph) {
  const found = new Set();
  const visit = (name) => {
    if (found.has(name) || !name.startsWith("quiddity-")) return;
    found.add(name);
    for (const dep of graph.get(name) ?? []) visit(dep);
  };
  for (const [, crate] of source.matchAll(/\bquiddity_(\w+)/g)) {
    visit(`quiddity-${crate}`);
  }
  return found;
}

/** Whether a changed file affects the target, given the crates it reaches. */
export function affects(file, target, crates) {
  if (SHARED.some((pattern) => pattern.test(file))) return true;
  if (file === `fuzz/fuzz_targets/${target}.rs`) return true;
  if (file.startsWith(`fuzz/seeds/${target}/`)) return true;
  // crates/core is the package quiddity-core.
  return [...crates].some((name) => file.startsWith(`crates/${name.slice("quiddity-".length)}/`));
}

/** The targets a set of changed files affects; `null` for "can't tell". */
export function affectedTargets(files, targets, graph) {
  if (files === null) return Object.keys(targets);
  return Object.entries(targets)
    .filter(([target, source]) => {
      const crates = reachableCrates(source, graph);
      return files.some((file) => affects(file, target, crates));
    })
    .map(([target]) => target);
}

function main(range) {
  const dir = new URL("../fuzz/fuzz_targets/", import.meta.url);
  const targets = Object.fromEntries(
    readdirSync(dir)
      .filter((name) => name.endsWith(".rs"))
      .sort()
      .map((name) => [name.slice(0, -3), readFileSync(new URL(name, dir), "utf8")]),
  );
  const lock = readFileSync(fileURLToPath(new URL("../fuzz/Cargo.lock", import.meta.url)), "utf8");

  let files = null;
  if (range) {
    try {
      files = execFileSync("git", ["diff", "--name-only", range], {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "ignore"],
      })
        .split("\n")
        .filter(Boolean);
    } catch {
      console.error(`can't diff ${range}: fuzzing every target`);
    }
  }
  console.log(JSON.stringify(affectedTargets(files, targets, lockedDependencies(lock))));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main(process.argv[2]);
