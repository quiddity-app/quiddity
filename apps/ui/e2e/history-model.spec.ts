import { expect, test } from '@playwright/test';

import { PlaceHistory, placeAt, type Place } from '../lib/history';

const at = (main: string | null): Place => ({
  path: main ? '/' : '/journals/',
  main,
});

function visited(...mains: (string | null)[]) {
  const history = new PlaceHistory();
  for (const m of mains) {
    history.observe(at(m));
  }
  return history;
}

test('back and forward walk what was visited, and going elsewhere forgets forward', () => {
  const history = visited('a', 'b', 'c');
  expect(history.back()).toEqual(at('b'));
  expect(history.back()).toEqual(at('a'));
  expect(history.back()).toBeUndefined();
  expect(history.forward()).toEqual(at('b'));
  history.observe(at('d'));
  expect(history.forward()).toBeUndefined();
  expect(history.back()).toEqual(at('b'));
});

// The pane is seen after it renders, which can be after the next key press.
test("a late sighting of the place just left doesn't forget what was forward", () => {
  const history = visited('a', 'b', 'c');
  expect(history.back()).toEqual(at('b'));
  // The second step is taken before "b" has been seen, so "c" is seen late.
  expect(history.back()).toEqual(at('a'));
  history.observe(at('c'));
  history.observe(at('b'));
  history.observe(at('a'));
  expect(history.forward()).toEqual(at('b'));
  expect(history.forward()).toEqual(at('c'));
});

test('once the place stepped to is seen, the place left is an ordinary visit again', () => {
  const history = visited('a', 'b');
  expect(history.back()).toEqual(at('a'));
  history.observe(at('a'));
  history.observe(at('b'));
  expect(history.forward()).toBeUndefined();
  expect(history.back()).toEqual(at('a'));
});

// The page's first place is known from its address before anything has rendered.
test('an address is the place the pane will later be seen showing', () => {
  expect(placeAt('/all/', '')).toEqual({ path: '/all/', main: null });
  expect(placeAt('/journals/', '?open=x')).toEqual({
    path: '/journals/',
    main: null,
  });
  expect(placeAt('/entity/', '?id=~favourite&open=a,b')).toEqual({
    path: '/',
    main: '~favourite',
  });
});

test("an address that only redirects, or needs the graph, isn't a place yet", () => {
  expect(placeAt('/', '')).toBeUndefined();
  expect(placeAt('/entity/', '')).toBeUndefined();
  expect(placeAt('/page/Some%20page/', '')).toBeUndefined();
});

test('a history begun at the address can go back from the first link followed', () => {
  const history = new PlaceHistory();
  history.observe(placeAt('/all/', '')!);
  history.observe(placeAt('/entity/', '?id=a')!);
  expect(history.back()).toEqual({ path: '/all/', main: null });
});

test('the pane showing the address it opened at is not a second visit', () => {
  const history = new PlaceHistory();
  history.observe(placeAt('/all/', '')!);
  history.observe({ path: '/all/', main: null });
  expect(history.back()).toBeUndefined();
});
