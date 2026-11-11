#!/usr/bin/env node
// Makes `latest.json`, the feed the app's updater reads, from the signed
// update bundles a release has built. See docs/Releases.md.
//
// Tauri signs each bundle it makes for updating (`createUpdaterArtifacts`) and
// leaves the signature beside it as `<bundle>.sig`. The updater wants, for each
// platform, the URL of the bundle and its signature, so this reads the
// signatures in a directory and works out which platform each bundle is for
// from its name:
//
//     Quiddity_1.2.3_amd64.AppImage        linux-x86_64
//     Quiddity_1.2.3_aarch64.AppImage      linux-aarch64
//     Quiddity_1.2.3_x64.app.tar.gz        darwin-x86_64
//     Quiddity_1.2.3_aarch64.app.tar.gz    darwin-aarch64
//     Quiddity_1.2.3_x64-setup.exe         windows-x86_64
//     Quiddity_1.2.3_arm64-setup.exe       windows-aarch64
//
// (The macOS bundle is named the same for both architectures, so the release
// renames it first.) Signatures for anything else, such as the `.msi` that
// Windows offers too, are left out. A platform with no bundle is an error: a
// feed that silently lacked one would leave those users on the old version.
//
//     node scripts/updater-manifest.mjs --dir dist --version 1.2.3 \
//         --tag v1.2.3 --repo owner/name [--notes text] [--date iso8601]

import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

export const PLATFORMS = [
  "linux-x86_64",
  "linux-aarch64",
  "darwin-x86_64",
  "darwin-aarch64",
  "windows-x86_64",
  "windows-aarch64",
];

const ARCH = { amd64: "x86_64", x64: "x86_64", x86_64: "x86_64", aarch64: "aarch64", arm64: "aarch64" };

// What an updater bundle's name ends with, per operating system.
const BUNDLES = [
  { os: "linux", suffix: ".AppImage" },
  { os: "darwin", suffix: ".app.tar.gz" },
  { os: "windows", suffix: "-setup.exe" },
];

/** The platform a bundle is for, or undefined if it isn't one the updater uses. */
export function platformOf(name) {
  for (const { os, suffix } of BUNDLES) {
    if (!name.endsWith(suffix)) continue;
    const arch = /_(amd64|x64|x86_64|aarch64|arm64)$/.exec(name.slice(0, -suffix.length))?.[1];
    if (arch === undefined) throw new Error(`can't tell which architecture ${name} is for`);
    return `${os}-${ARCH[arch]}`;
  }
  return undefined;
}

/**
 * The feed's contents. `files` maps each signature's file name to its text.
 * `url(name)` says where a bundle will be downloaded from.
 */
export function manifest({ files, version, notes, date, url }) {
  const platforms = {};
  for (const [sig, signature] of Object.entries(files)) {
    if (!sig.endsWith(".sig")) continue;
    const name = sig.slice(0, -".sig".length);
    const platform = platformOf(name);
    if (platform === undefined) continue;
    if (platform in platforms)
      throw new Error(`two bundles for ${platform}: ${platforms[platform].name} and ${name}`);
    platforms[platform] = { name, signature: signature.trim(), url: url(name) };
  }
  const missing = PLATFORMS.filter((platform) => !(platform in platforms));
  if (missing.length > 0) throw new Error(`no signed bundle for ${missing.join(", ")}`);
  return {
    version,
    notes,
    pub_date: date,
    // The updater reads `signature` and `url`; the name was for the errors.
    platforms: Object.fromEntries(
      PLATFORMS.map((platform) => [
        platform,
        { signature: platforms[platform].signature, url: platforms[platform].url },
      ]),
    ),
  };
}

function main() {
  const { values } = parseArgs({
    options: {
      dir: { type: "string" },
      version: { type: "string" },
      tag: { type: "string" },
      repo: { type: "string" },
      notes: { type: "string", default: "" },
      date: { type: "string", default: new Date().toISOString() },
    },
  });
  for (const required of ["dir", "version", "tag", "repo"]) {
    if (!values[required]) throw new Error(`--${required} is required`);
  }
  const files = Object.fromEntries(
    readdirSync(values.dir)
      .filter((name) => name.endsWith(".sig"))
      .map((name) => [name, readFileSync(join(values.dir, name), "utf8")]),
  );
  const feed = manifest({
    files,
    version: values.version,
    notes: values.notes,
    date: values.date,
    url: (name) =>
      `https://github.com/${values.repo}/releases/download/${encodeURIComponent(values.tag)}/${encodeURIComponent(name)}`,
  });
  writeFileSync(join(values.dir, "latest.json"), `${JSON.stringify(feed, null, 2)}\n`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) main();
