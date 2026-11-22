import { expect, test } from '@playwright/test';

import { share } from '../lib/share';

// No page needed: this is `share` on its own, in Node.

/** A small deterministic generator, so a failure can be replayed. */
function random(seed: number) {
  let state = seed;
  return () => {
    state = (state * 1_664_525 + 1_013_904_223) >>> 0;
    return state / 2 ** 32;
  };
}

/** A made-up answer like the replica's: nested arrays and objects, with few
 * keys and values so that two of them often agree in part. */
function value(next: () => number, depth = 0): unknown {
  const pick = next();
  if (depth > 3 || pick < 0.3) {
    return [1, 'a', null, true][Math.floor(next() * 4)];
  }
  if (pick < 0.65) {
    return Array.from({ length: Math.floor(next() * 4) }, () =>
      value(next, depth + 1),
    );
  }
  const object: Record<string, unknown> = {};
  for (const key of ['id', 'text', 'children', 'links']) {
    if (next() < 0.6) {
      object[key] = value(next, depth + 1);
    }
  }
  return object;
}

test('what comes back always equals what was read, and an equal read is the same object', () => {
  const next = random(7);
  for (let i = 0; i < 300; i++) {
    const before = value(next);
    const after = value(next);
    expect(share(before, after)).toEqual(after);
    // Equal, but made afresh, as a second read of the replica would be.
    const again = structuredClone(before);
    if (typeof before === 'object' && before !== null) {
      expect(share(before, again)).toBe(before);
    }
  }
});

test('only the part that changed is new', () => {
  const before = {
    title: 'Loro',
    blocks: [
      { id: '1', text: 'a' },
      { id: '2', text: 'b' },
    ],
    links: { x: 1 },
  };
  const after = {
    title: 'Loro',
    blocks: [
      { id: '1', text: 'a' },
      { id: '2', text: 'B' },
    ],
    links: { x: 1 },
  };
  const shared = share(before, after);
  expect(shared).toEqual(after);
  expect(shared).not.toBe(before);
  expect(shared.blocks).not.toBe(before.blocks);
  expect(shared.blocks[0]).toBe(before.blocks[0]);
  expect(shared.blocks[1]).not.toBe(before.blocks[1]);
  expect(shared.links).toBe(before.links);
});

test('blocks added, removed or moved keep the ones that are still equal', () => {
  const a = { id: '1' };
  const b = { id: '2' };
  const before = [a, b];
  // Compared place by place: a block that moved is a different one in its place.
  const moved = share(before, [{ id: '2' }, { id: '1' }]);
  expect(moved).toEqual([b, a]);
  expect(moved[0]).not.toBe(a);
  const longer = share(before, [{ id: '1' }, { id: '2' }, { id: '3' }]);
  expect(longer[0]).toBe(a);
  expect(longer[1]).toBe(b);
  expect(longer).not.toBe(before);
  const shorter = share(before, [{ id: '1' }]);
  expect(shorter[0]).toBe(a);
  expect(shorter).toHaveLength(1);
});

test('a key that comes or goes makes a new object, and a kind that changes is taken as it is', () => {
  const before = { a: { n: 1 } };
  expect(share(before, { a: { n: 1 }, b: null }).a).toBe(before.a);
  expect(share(before, { a: { n: 1 }, b: null })).not.toBe(before);
  expect(share({ a: 1, b: 2 }, { a: 1 })).toEqual({ a: 1 });
  expect(share({ a: 1 }, [1])).toEqual([1]);
  expect(share([1], { a: 1 })).toEqual({ a: 1 });
  expect(share(1n, 1n)).toBe(1n);
  expect(share(undefined, { a: 1 })).toEqual({ a: 1 });
});
