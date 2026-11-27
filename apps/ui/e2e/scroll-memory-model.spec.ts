import { expect, test } from '@playwright/test';

import { ScrollMemory } from '../lib/scroll-memory';

import type { Place } from '../lib/history';

const at = (main: string | null, path = '/entity/'): Place => ({ path, main });

test('a place comes back as it was left, and a new one starts at the top', () => {
  const memory = new ScrollMemory();
  expect(memory.arrive(at('a'))).toBe(0);
  memory.scrolled(300);
  expect(memory.arrive(at('b'))).toBe(0);
  memory.scrolled(40);
  expect(memory.arrive(at('a'))).toBe(300);
  expect(memory.arrive(at('b'))).toBe(40);
});

test('arriving where it already is changes nothing, whatever the side panel does', () => {
  const memory = new ScrollMemory();
  memory.arrive(at('a'));
  memory.scrolled(120);
  expect(memory.arrive(at('a'))).toBe(120);
  memory.scrolled(150);
  expect(memory.arrive(at('b'))).toBe(0);
  expect(memory.arrive(at('a'))).toBe(150);
});

test('the page and the entity both identify a place', () => {
  const memory = new ScrollMemory();
  memory.arrive(at(null, '/journals/'));
  memory.scrolled(500);
  memory.arrive(at(null, '/graph/'));
  expect(memory.arrive(at(null, '/journals/'))).toBe(500);
  memory.arrive(at('a'));
  expect(memory.arrive(at('a', '/other/'))).toBe(0);
});

test('only the most recent places are kept', () => {
  const memory = new ScrollMemory(2);
  for (const [name, top] of [
    ['a', 1],
    ['b', 2],
    ['c', 3],
  ] as const) {
    memory.arrive(at(name));
    memory.scrolled(top);
  }
  memory.arrive(at('d'));
  expect(memory.arrive(at('c'))).toBe(3);
  expect(memory.arrive(at('a'))).toBe(0);
});

test('visiting a place again makes it the newest, not the next to go', () => {
  const memory = new ScrollMemory(2);
  memory.arrive(at('a'));
  memory.scrolled(1);
  memory.arrive(at('b'));
  memory.scrolled(2);
  memory.arrive(at('a'));
  memory.scrolled(11);
  memory.arrive(at('c'));
  memory.arrive(at('d'));
  expect(memory.arrive(at('a'))).toBe(0);
});

test('a seeded place is found where it was left, as if it had been visited', () => {
  const memory = new ScrollMemory();
  memory.seed(at('a'), 220);
  expect(memory.arrive(at('b'))).toBe(0);
  expect(memory.arrive(at('a'))).toBe(220);
  // Seeding is not arriving: the first place shown is still the top for one never seen.
  const fresh = new ScrollMemory();
  fresh.seed(at('a'), 220);
  expect(fresh.arrive(at('c'))).toBe(0);
});
