import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { parseJournal, renderDay, rollUp, splitSubject, ROLLUP_SUBJECT } from "./journals.mjs";

test("a journal block runs to the first line that isn't an outline line", () => {
  const msg = "Title\n\nProse.\n\nJournal:\n- Did a thing. [[Foo]]\n\t- Because.\n  continued\n\nCo-Authored-By: x\n";
  assert.deepEqual(parseJournal(msg), ["- Did a thing. [[Foo]]", "\t- Because.", "  continued"]);
});

test("a message without a block, or with prose after the marker, has no journal", () => {
  assert.deepEqual(parseJournal("Title\n\n- a bullet\n"), []);
  assert.deepEqual(parseJournal("Title\n\nJournal:\nnot an outline\n"), []);
  assert.deepEqual(parseJournal("Title\n\nJournal:\n\t- indented first\n"), []);
  assert.deepEqual(parseJournal("Title\n\nJournal: something\n- x\n"), []);
});

test("CRLF and trailing spaces don't reach the page", () => {
  assert.deepEqual(parseJournal("T\r\n\r\nJournal:\r\n- a  \r\n\t- b\r\n"), ["- a", "\t- b"]);
});

test("GitHub's (#n) suffix is the link, not the title", () => {
  assert.deepEqual(splitSubject("Fix it (#12)"), { title: "Fix it", number: 12 });
  assert.deepEqual(splitSubject("Fix (the) thing"), { title: "Fix (the) thing", number: null });
});

test("the journal sits one level under its title", () => {
  const text = renderDay([
    { title: "A", url: "u/1", label: "#1", journal: ["- x", "\t- y"] },
    { title: "B", url: "u/2", label: "#2", journal: [] },
  ]);
  assert.equal(text, "- A ([#1](u/1))\n\t- x\n\t\t- y\n- B ([#2](u/2))\n");
});

test("only days after the newest entry and before today are rolled up; roll-ups are skipped", () => {
  const c = (sha, date, subject, journal = "") => ({
    sha: sha.padEnd(40, "0"), date, subject, message: `${subject}\n\n${journal}`,
  });
  const commits = [
    c("a", "2026-11-01", "Old (#1)"),
    c("b", "2026-11-02", "Kept (#2)", "Journal:\n- Hi\n"),
    c("c", "2026-11-02", "No pull request"),
    c("d", "2026-11-03", `${ROLLUP_SUBJECT} up to 2026-11-02`),
    c("e", "2026-11-04", "Today (#4)"),
  ];
  const pages = rollUp({ commits, after: "2026-11-01", before: "2026-11-04", repo: "o/r", resolve: () => null });
  assert.deepEqual([...pages.keys()], ["2026-11-02"]);
  assert.equal(
    pages.get("2026-11-02"),
    "- Kept ([#2](https://github.com/o/r/pull/2))\n\t- Hi\n- No pull request ([c000000](https://github.com/o/r/commit/c000000000000000000000000000000000000000))\n",
  );
});

test("end to end, in a scratch repository", () => {
  const dir = mkdtempSync(join(tmpdir(), "journals-"));
  const git = (...a) => execFileSync("git", a, { cwd: dir, env: { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null" } });
  git("init", "-q", "-b", "main");
  git("config", "user.name", "t");
  git("config", "user.email", "t@example.com");
  mkdirSync(join(dir, "docs/journals"), { recursive: true });
  writeFileSync(join(dir, "docs/journals/2026-11-01.md"), "- old\n");
  const commit = (date, msg) => {
    writeFileSync(join(dir, "f"), msg);
    git("add", "-A");
    execFileSync("git", ["commit", "-q", "-m", msg], {
      cwd: dir,
      env: { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_COMMITTER_DATE: date, GIT_AUTHOR_DATE: date },
    });
  };
  commit("2026-11-01T10:00:00Z", "Before (#1)");
  commit("2026-11-02T23:30:00Z", "Landed (#2)\n\nJournal:\n- Did it\n");
  commit("2026-11-03T00:30:00Z", "Next day (#3)");
  const script = fileURLToPath(new URL("./journals.mjs", import.meta.url));
  execFileSync("node", [script, "--repo", "o/r", "--dir", join(dir, "docs/journals")], { cwd: dir, env: { ...process.env, TZ: "Asia/Tokyo" } });
  assert.deepEqual(readdirSync(join(dir, "docs/journals")), ["2026-11-01.md", "2026-11-02.md", "2026-11-03.md"]);
  assert.equal(readFileSync(join(dir, "docs/journals/2026-11-02.md"), "utf8"), "- Landed ([#2](https://github.com/o/r/pull/2))\n\t- Did it\n");
  assert.equal(readFileSync(join(dir, "docs/journals/2026-11-01.md"), "utf8"), "- old\n");
});
