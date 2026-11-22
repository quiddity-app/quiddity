import { expect, test } from '@playwright/test';

import { debounce } from '../lib/debounce';

// No page needed: this is the debounce on its own, in Node.

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

test('a burst of calls is one run, once it has been quiet', async () => {
  let runs = 0;
  const d = debounce(() => runs++, 40, 1000);
  for (let i = 0; i < 5; i++) {
    d.call();
    await sleep(10);
  }
  expect(runs).toBe(0);
  expect(d.pending).toBe(true);
  await expect.poll(() => runs).toBe(1);
  expect(d.pending).toBe(false);
});

test('calls that never pause still run it, every `max`', async () => {
  let runs = 0;
  // A call every 10 ms never leaves the 50 ms of quiet a plain debounce wants.
  const d = debounce(() => runs++, 50, 150);
  const end = Date.now() + 700;
  while (Date.now() < end) {
    d.call();
    await sleep(10);
  }
  // 700 ms is four and a bit of 150 ms; allow for a slow machine.
  expect(runs).toBeGreaterThanOrEqual(3);
  d.cancel();
});

test("`max` counts from the first call it hasn't answered, not from the last run", async () => {
  let runs = 0;
  const d = debounce(() => runs++, 30, 100);
  d.call();
  await expect.poll(() => runs).toBe(1);
  // Long after, a new call is a new burst: it waits for the quiet, not for `max`.
  await sleep(250);
  d.call();
  expect(runs).toBe(1);
  await expect.poll(() => runs).toBe(2);
});

test("cancel forgets a call that hasn't run", async () => {
  let runs = 0;
  const d = debounce(() => runs++, 20, 100);
  d.call();
  d.cancel();
  expect(d.pending).toBe(false);
  await sleep(80);
  expect(runs).toBe(0);
});
