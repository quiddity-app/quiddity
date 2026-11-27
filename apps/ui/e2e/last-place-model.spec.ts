import { expect, test } from '@playwright/test';

import { decode, encode, RESUME_MS } from '../lib/last-place';

const now = 1_700_000_000_000;
const kept = (href: string, top = 0, at = now) =>
  JSON.stringify({ href, top, at });

test('a place comes back as it was kept', () => {
  const last = { href: '/entity/?id=1&open=2,3', top: 240 };
  expect(decode(encode(last, now), now + 1000)).toEqual(last);
});

test('a place is only gone back to for so long', () => {
  const text = kept('/all/');
  expect(decode(text, now + RESUME_MS)).toEqual({ href: '/all/', top: 0 });
  expect(decode(text, now + RESUME_MS + 1)).toBeUndefined();
});

test('a place from the future, as when the clock was put back, is not trusted', () => {
  expect(decode(kept('/all/', 0, now + 1), now)).toBeUndefined();
});

test('nothing kept, or what is not ours, is no place', () => {
  expect(decode(null, now)).toBeUndefined();
  expect(decode('not json', now)).toBeUndefined();
  expect(decode('null', now)).toBeUndefined();
  expect(decode('"/all/"', now)).toBeUndefined();
  expect(
    decode(JSON.stringify({ href: '/all/', top: 0 }), now),
  ).toBeUndefined();
  expect(decode(kept('/all/', -1), now)).toBeUndefined();
  expect(
    decode(JSON.stringify({ href: '/all/', top: '3', at: now }), now),
  ).toBeUndefined();
  expect(
    decode(JSON.stringify({ href: 3, top: 0, at: now }), now),
  ).toBeUndefined();
});

test('only an address of the app that is a place is followed', () => {
  // Another site's, however it is spelt.
  expect(decode(kept('//example.com/all/'), now)).toBeUndefined();
  expect(decode(kept('https://example.com/all/'), now)).toBeUndefined();
  expect(decode(kept('all/'), now)).toBeUndefined();
  // `/` only redirects, and a published page's name needs the graph.
  expect(decode(kept('/'), now)).toBeUndefined();
  expect(decode(kept('/?open=1'), now)).toBeUndefined();
  expect(decode(kept('/page/Quiddity/'), now)).toBeUndefined();
  // Entities, and the pages that need none.
  expect(decode(kept('/entity/?id=1'), now)).toBeDefined();
  expect(decode(kept('/journals/'), now)).toBeDefined();
  expect(decode(kept('/graph/?open=4'), now)).toBeDefined();
});
