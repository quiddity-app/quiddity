import { readFileSync } from "node:fs";
import { expect, test } from "./fixtures";

// The app's icon is a Q whose ring, not the whole glyph, is centred in the
// square: on a circular base (an Android round icon, a mask) it is the ring
// that reads as the shape, and a glyph centred by its tail leaves the ring off
// to one side. These read the source SVGs, which every size is made from.

const source = (name: string) =>
  readFileSync(new URL(`../../app/src-tauri/icons/source/${name}`, import.meta.url), "utf8");

const number = (svg: string, pattern: RegExp) => Number(pattern.exec(svg)?.[1]);
const ring = (svg: string) => ({
  cx: number(svg, /<circle[^>]*\scx="([-\d.]+)"/),
  cy: number(svg, /<circle[^>]*\scy="([-\d.]+)"/),
  r: number(svg, /<circle[^>]*\sr="([-\d.]+)"/),
});

test("the master's ring is centred in its canvas", () => {
  const svg = source("icon.svg");
  const [, , width, height] = /viewBox="([-\d.\s]+)"/.exec(svg)![1]!.split(/\s+/).map(Number);
  expect(ring(svg).cx).toBe(width! / 2);
  expect(ring(svg).cy).toBe(height! / 2);
});

for (const name of ["icon-foreground.svg", "icon-monochrome.svg"]) {
  test(`${name} puts the ring in the middle too, and is the master's shape`, () => {
    const master = ring(source("icon.svg"));
    const svg = source(name);
    // translate(middle) scale(s) translate(-ring centre): the ring lands in the middle.
    const placed =
      /translate\(([-\d.]+) ([-\d.]+)\) scale\(([\d.]+)\) translate\(([-\d.]+) ([-\d.]+)\)/.exec(svg)!;
    const [middleX, middleY, , backX, backY] = placed.slice(1).map(Number);
    expect([middleX, middleY]).toEqual([512, 512]);
    expect([backX, backY]).toEqual([-master.cx, -master.cy]);
    expect(ring(svg)).toEqual(master);
  });
}

test("the macOS icon is the master's ring, scaled about the middle", () => {
  const master = ring(source("icon.svg"));
  const svg = source("icon-macos.svg");
  const placed =
    /translate\(([-\d.]+) ([-\d.]+)\) scale\(([\d.]+)\) translate\(([-\d.]+) ([-\d.]+)\)/.exec(svg)!;
  const [middleX, middleY, scale, backX, backY] = placed.slice(1).map(Number);
  expect([middleX, middleY]).toEqual([512, 512]);
  expect([backX, backY]).toEqual([-master.cx, -master.cy]);
  expect(ring(svg)).toEqual(master);
  // Apple's template: an 824px body on the 1024px canvas.
  expect(scale! * 1024).toBeCloseTo(824, 0);
});

test("the page has an icon, and it loads", async ({ page, request, baseURL }) => {
  await page.goto("/devices/");
  const href = await page.locator('link[rel~="icon"]').first().getAttribute("href");
  expect(href, "an icon link").toBeTruthy();
  const response = await request.get(new URL(href!, baseURL).toString());
  expect(response.ok()).toBe(true);
  expect(response.headers()["content-type"]).toContain("svg");
});

// The same, in what is made from them: Tauri composes the layers into the
// older launcher icons itself, and has put the Q's ring off to one side.
const densities = ["mdpi", "hdpi", "xhdpi", "xxhdpi", "xxxhdpi"];
const launcherIcons = densities.flatMap((density) =>
  ["ic_launcher", "ic_launcher_round", "ic_launcher_foreground"].map(
    (name) => `android/mipmap-${density}/${name}.png`,
  ),
);

for (const file of launcherIcons) {
  test(`${file} has the ring in the middle`, async ({ page }) => {
    const png = readFileSync(new URL(`../../app/src-tauri/icons/${file}`, import.meta.url)).toString("base64");
    const edges = await page.evaluate(async (png) => {
      const image = new Image();
      image.src = `data:image/png;base64,${png}`;
      await image.decode();
      const canvas = new OffscreenCanvas(image.width, image.height);
      const context = canvas.getContext("2d")!;
      context.drawImage(image, 0, 0);
      const light = (x: number, y: number) => {
        const [red = 0, , , alpha = 0] = context.getImageData(x, y, 1, 1).data;
        return alpha > 128 && red > 150;
      };
      const along = (at: (i: number) => boolean) => {
        const found: number[] = [];
        for (let i = 1; i < image.width; i++) if (at(i) !== at(i - 1)) found.push(i);
        return found;
      };
      const middle = Math.floor(image.width / 2);
      return {
        size: image.width,
        row: along((x) => light(x, middle)),
        column: along((y) => light(middle, y)),
      };
    }, png);
    // The ring's outer edges are the first and fourth crossings of the middle line.
    for (const crossings of [edges.row, edges.column]) {
      expect(crossings.length, "outer and inner edge of the ring, both sides").toBeGreaterThanOrEqual(4);
      const centre = (crossings[0]! + crossings[3]!) / 2;
      expect(Math.abs(centre - edges.size / 2)).toBeLessThanOrEqual(1);
    }
  });
}
