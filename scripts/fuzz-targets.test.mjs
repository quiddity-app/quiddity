import assert from "node:assert/strict";
import { test } from "node:test";
import { affectedTargets, lockedDependencies, reachableCrates } from "./fuzz-targets.mjs";

const lock = `
version = 4

[[package]]
name = "quiddity-core"
version = "0.1.0"
dependencies = [
  "jiff",
  "quiddity-syntax",
  "thiserror 2.0.21",
]

[[package]]
name = "quiddity-fuzz"
version = "0.0.0"
dependencies = [
  "quiddity-core",
]

[[package]]
name = "quiddity-sync"
version = "0.1.0"
dependencies = [
  "quiddity-core",
]

[[package]]
name = "quiddity-syntax"
version = "0.1.0"

[[package]]
name = "serde"
version = "1.0.0"
`;
const graph = lockedDependencies(lock);
const targets = {
  block: "use quiddity_syntax::parse;",
  outline: "use quiddity_core::outline::format_outline;",
  peer: "use quiddity_core::Graph;\nuse quiddity_sync::Session;",
};

test("the lock gives each package its dependencies, without versions", () => {
  assert.deepEqual(graph.get("quiddity-core"), ["jiff", "quiddity-syntax", "thiserror"]);
  assert.deepEqual(graph.get("quiddity-syntax"), []);
});

test("a target reaches the crates it names and what they depend on", () => {
  assert.deepEqual([...reachableCrates(targets.block, graph)], ["quiddity-syntax"]);
  assert.deepEqual(
    [...reachableCrates(targets.outline, graph)].sort(),
    ["quiddity-core", "quiddity-syntax"],
  );
  assert.deepEqual(
    [...reachableCrates(targets.peer, graph)].sort(),
    ["quiddity-core", "quiddity-sync", "quiddity-syntax"],
  );
});

const affected = (...files) => affectedTargets(files, targets, graph);

test("a change to a crate affects the targets that reach it, however it is reached", () => {
  assert.deepEqual(affected("crates/syntax/src/lib.rs"), ["block", "outline", "peer"]);
  assert.deepEqual(affected("crates/core/src/graph.rs"), ["outline", "peer"]);
  assert.deepEqual(affected("crates/sync/tests/net.rs"), ["peer"]);
});

test("a crate no target reaches affects none, and a prefix is not a match", () => {
  assert.deepEqual(affected("crates/view/src/lib.rs", "apps/ui/src/page.tsx", "docs/Testing.md"), []);
  assert.deepEqual(affected("crates/syntax-extras/src/lib.rs"), []);
});

test("a target's own source and seeds affect only that target", () => {
  assert.deepEqual(affected("fuzz/fuzz_targets/peer.rs"), ["peer"]);
  assert.deepEqual(affected("fuzz/seeds/block/crash-1"), ["block"]);
});

test("what the targets share affects them all", () => {
  for (const file of ["fuzz/Cargo.lock", "Cargo.toml", "mise.toml", ".github/workflows/ci.yml", ".github/actions/setup/action.yml"]) {
    assert.deepEqual(affected(file), ["block", "outline", "peer"], file);
  }
});

test("when the change can't be known, everything is fuzzed", () => {
  assert.deepEqual(affectedTargets(null, targets, graph), ["block", "outline", "peer"]);
});
