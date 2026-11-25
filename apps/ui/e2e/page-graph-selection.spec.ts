import { expect, test } from '@playwright/test';

import { pageGraph } from '../lib/page-graph';

import type { Connections } from '../lib/model';

// No page needed: this is `pageGraph` on its own, in Node.

const edge = (source: string, target: string, written = true) => ({
  source,
  target,
  weak: false,
  written,
});

/** The page `P`, neighbours `A` and `B`, and a far entity `F`, with whatever
 * links to `F` the test gives. */
function graph(...toFar: Connections['edges']): Connections {
  return {
    nodes: ['P', 'A', 'B', 'F'].map((id) => ({
      id,
      title: id,
      journal: false,
    })),
    edges: [edge('P', 'A'), edge('P', 'B'), ...toFar],
  };
}

const drawn = (all: Connections) =>
  pageGraph(all, 'P')
    .nodes.map((n) => n.id)
    .sort();

test('two steps away is kept when two neighbours link to it in their text', () => {
  expect(drawn(graph(edge('A', 'F'), edge('B', 'F')))).toEqual([
    'A',
    'B',
    'F',
    'P',
  ]);
});

test("a neighbour's properties block doesn't tie a far entity in", () => {
  const all = graph(edge('A', 'F'), edge('B', 'F', false));
  expect(drawn(all)).toEqual(['A', 'B', 'P']);
  expect(drawn(graph(edge('A', 'F', false), edge('B', 'F', false)))).toEqual([
    'A',
    'B',
    'P',
  ]);
});

test("the far entity's own properties block does tie it in", () => {
  expect(drawn(graph(edge('F', 'A', false), edge('B', 'F')))).toEqual([
    'A',
    'B',
    'F',
    'P',
  ]);
});

test("one neighbour linking both ways doesn't count twice", () => {
  expect(drawn(graph(edge('A', 'F'), edge('F', 'A')))).toEqual(['A', 'B', 'P']);
});

test('every link between what is drawn is kept, whether it counted or not', () => {
  const kept = pageGraph(
    graph(edge('A', 'F'), edge('B', 'F'), edge('A', 'B', false)),
    'P',
  );
  expect(kept.edges).toHaveLength(5);
});
