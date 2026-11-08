#!/usr/bin/env node
// Rolls the journal entries that commit messages carry up into docs/journals.
//
// A commit (and so its pull request) says what it did for the day's journal in
// a block in its message, so that no two changes edit the same file:
//
//     Journal:
//     - Moved blobs between devices. [[Blob transfer]]
//     	- A child, indented with a tab, as in any page.
//
// The block starts at a line that is exactly `Journal:` and runs over the
// lines after it that begin `- ` or are indented, to the first blank line or
// any other line. See docs/Journal entries.md.
//
// For each day after the newest file in the journals directory, the commits
// that landed on the branch that day are listed as a top-level bullet each: the
// pull request's title and link, with the commit's journal block under it. A
// day is the UTC date the commit was committed, which on main is the day it
// landed.
//
//     node scripts/journals.mjs [--dir docs/journals] [--ref HEAD]
//                               [--repo owner/name] [--exclude-today]

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

// The subject of the commit that saves the journals back. It is left out of
// the entries, or each roll-up would be listed in the next day's.
export const ROLLUP_SUBJECT = "Roll up journal entries";

const DATE_FILE = /^(\d{4}-\d{2}-\d{2})\.md$/;

/** The outline lines of a message's journal block, or [] if it has none. */
export function parseJournal(message) {
  const lines = message.replace(/\r/g, "").split("\n");
  const start = lines.findIndex((l) => l.trimEnd() === "Journal:");
  if (start < 0) return [];
  const block = [];
  for (const line of lines.slice(start + 1)) {
    if (!/^(- |[ \t]+\S)/.test(line)) break;
    block.push(line.trimEnd());
  }
  // A block must begin with a top-level bullet; anything else is prose.
  return block.length > 0 && block[0].startsWith("- ") ? block : [];
}

/** A commit's subject without the ` (#123)` GitHub appends when squashing. */
export function splitSubject(subject) {
  const m = /^(.*?)\s*\(#(\d+)\)$/.exec(subject.trim());
  return m ? { title: m[1], number: Number(m[2]) } : { title: subject.trim(), number: null };
}

/** One day's page: a bullet per commit, with its journal under the title. */
export function renderDay(entries) {
  const out = [];
  for (const { title, url, label, journal } of entries) {
    out.push(`- ${title} ([${label}](${url}))`);
    for (const line of journal) out.push(`\t${line}`);
  }
  return out.join("\n") + "\n";
}

function git(args, env = {}) {
  return execFileSync("git", args, {
    encoding: "utf8",
    maxBuffer: 1 << 28,
    env: { ...process.env, ...env },
  });
}

function repoFromRemote() {
  if (process.env.GITHUB_REPOSITORY) return process.env.GITHUB_REPOSITORY;
  const url = git(["remote", "get-url", "origin"]).trim();
  const m = /github\.com[:/]([^/]+\/[^/]+?)(?:\.git)?$/.exec(url);
  if (!m) throw new Error(`cannot tell the GitHub repository from ${url}; pass --repo`);
  return m[1];
}

// The pull request a commit came from, when the subject doesn't say. Needs
// `gh` and a token; without them the entry links to the commit instead.
function lookUpPull(repo, sha) {
  if (!process.env.GH_TOKEN) return null;
  try {
    const out = execFileSync(
      "gh",
      ["api", `repos/${repo}/commits/${sha}/pulls`, "--jq", ".[0] | [.number, .title] | @tsv"],
      { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] },
    ).trim();
    const [number, title] = out.split("\t");
    return number ? { number: Number(number), title } : null;
  } catch {
    return null;
  }
}

/** The commits on `ref` as { sha, date, subject, message }, oldest first. */
export function readCommits(ref) {
  // Dates are UTC whatever the machine's zone is.
  const raw = git(
    ["log", "--first-parent", "--reverse", "--date=format-local:%Y-%m-%d", "--format=%H%x1f%cd%x1f%B%x1e", ref],
    { TZ: "UTC" },
  );
  return raw
    .split("\x1e")
    .map((r) => r.replace(/^\n/, ""))
    .filter((r) => r.trim() !== "")
    .map((r) => {
      const [sha, date, message] = r.split("\x1f");
      return { sha, date, message, subject: message.split("\n")[0] };
    });
}

/** Maps each date to be written to its page's text. */
export function rollUp({ commits, after, before, repo, resolve = lookUpPull }) {
  const days = new Map();
  for (const c of commits) {
    if (after && c.date <= after) continue;
    if (before && c.date >= before) continue;
    if (c.subject.startsWith(ROLLUP_SUBJECT)) continue;
    let { title, number } = splitSubject(c.subject);
    if (number === null) {
      const pull = resolve(repo, c.sha);
      if (pull) ({ title, number } = { title: pull.title, number: pull.number });
    }
    const entry = {
      title,
      journal: parseJournal(c.message),
      ...(number === null
        ? { url: `https://github.com/${repo}/commit/${c.sha}`, label: c.sha.slice(0, 7) }
        : { url: `https://github.com/${repo}/pull/${number}`, label: `#${number}` }),
    };
    if (!days.has(c.date)) days.set(c.date, []);
    days.get(c.date).push(entry);
  }
  return new Map([...days].map(([d, es]) => [d, renderDay(es)]));
}

function main(argv) {
  const opts = { dir: "docs/journals", ref: "HEAD", repo: null, excludeToday: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--dir") opts.dir = argv[++i];
    else if (a === "--ref") opts.ref = argv[++i];
    else if (a === "--repo") opts.repo = argv[++i];
    else if (a === "--exclude-today") opts.excludeToday = true;
    else throw new Error(`unknown argument ${a}`);
  }
  mkdirSync(opts.dir, { recursive: true });
  const existing = readdirSync(opts.dir)
    .map((f) => DATE_FILE.exec(f)?.[1])
    .filter(Boolean)
    .sort();
  const after = existing.at(-1) ?? null;
  const before = opts.excludeToday ? new Date().toISOString().slice(0, 10) : null;
  const pages = rollUp({
    commits: readCommits(opts.ref),
    after,
    before,
    repo: opts.repo ?? repoFromRemote(),
  });
  for (const [date, text] of pages) {
    const file = join(opts.dir, `${date}.md`);
    if (existsSync(file)) continue;
    writeFileSync(file, text);
    console.log(`wrote ${file}`);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) main(process.argv.slice(2));
