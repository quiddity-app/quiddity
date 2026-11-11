import assert from "node:assert/strict";
import { test } from "node:test";
import { PLATFORMS, manifest, platformOf } from "./updater-manifest.mjs";

const bundles = [
  "Quiddity_1.2.3_amd64.AppImage",
  "Quiddity_1.2.3_aarch64.AppImage",
  "Quiddity_1.2.3_x64.app.tar.gz",
  "Quiddity_1.2.3_aarch64.app.tar.gz",
  "Quiddity_1.2.3_x64-setup.exe",
  "Quiddity_1.2.3_arm64-setup.exe",
];
const sigs = (names) => Object.fromEntries(names.map((name) => [`${name}.sig`, `sig of ${name}\n`]));
const url = (name) => `https://example.test/${name}`;
const make = (files) =>
  manifest({ files, version: "1.2.3", notes: "News", date: "2026-11-11T00:00:00Z", url });

test("each bundle is for the platform its name says", () => {
  assert.deepEqual(bundles.map(platformOf), [
    "linux-x86_64",
    "linux-aarch64",
    "darwin-x86_64",
    "darwin-aarch64",
    "windows-x86_64",
    "windows-aarch64",
  ]);
});

test("files the updater doesn't use have no platform", () => {
  assert.equal(platformOf("Quiddity_1.2.3_x64_en-US.msi"), undefined);
  assert.equal(platformOf("quiddity-node-1.2.3-x86_64-unknown-linux-gnu.tar.gz"), undefined);
});

test("a bundle whose architecture is unknown is an error, not a guess", () => {
  assert.throws(() => platformOf("Quiddity_1.2.3_riscv.AppImage"), /architecture/);
});

test("the feed names every platform's bundle and signature", () => {
  const feed = make(sigs(bundles));
  assert.equal(feed.version, "1.2.3");
  assert.equal(feed.notes, "News");
  assert.equal(feed.pub_date, "2026-11-11T00:00:00Z");
  assert.deepEqual(Object.keys(feed.platforms), PLATFORMS);
  assert.deepEqual(feed.platforms["darwin-aarch64"], {
    signature: "sig of Quiddity_1.2.3_aarch64.app.tar.gz",
    url: "https://example.test/Quiddity_1.2.3_aarch64.app.tar.gz",
  });
});

test("signatures for other files are left out", () => {
  const feed = make({ ...sigs(bundles), "Quiddity_1.2.3_x64_en-US.msi.sig": "msi" });
  assert.equal(Object.keys(feed.platforms).length, PLATFORMS.length);
});

test("a platform with no bundle stops the release", () => {
  assert.throws(() => make(sigs(bundles.slice(1))), /linux-x86_64/);
});

test("two bundles for one platform stop the release", () => {
  assert.throws(() => make(sigs([...bundles, "Quiddity_1.2.3_x64.AppImage"])), /two bundles/);
});
