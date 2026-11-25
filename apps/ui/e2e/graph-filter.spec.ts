import { expect, test } from '@playwright/test';

import { HIDE_ALL, filterConnections } from '../lib/graph-filter';

import type { Connections } from '../lib/model';

// No page needed: this is `filterConnections` on its own, in Node.

const node = (id: string, empty = false, journal = false) => ({
  id,
  title: id,
  empty,
  journal,
});
const edge = (source: string, target: string, weak = false) => ({
  source,
  target,
  weak,
  written: !weak,
});
const ids = (c: Connections) => c.nodes.map((n) => n.id).sort();

test('an empty page with one link in is hidden, and the link with it', () => {
  const data: Connections = {
    nodes: [node('A'), node('B'), node('E', true)],
    edges: [edge('A', 'B'), edge('A', 'E')],
  };
  const hidden = filterConnections(data, HIDE_ALL);
  expect(ids(hidden)).toEqual(['A', 'B']);
  expect(hidden.edges).toEqual([edge('A', 'B')]);
  const shown = filterConnections(data, { ...HIDE_ALL, emptyLeaves: true });
  expect(ids(shown)).toEqual(['A', 'B', 'E']);
});

test('a leaf with content, or with a second link, stays', () => {
  const data: Connections = {
    nodes: [node('A'), node('B'), node('C'), node('Full'), node('Two', true)],
    edges: [
      edge('A', 'Full'),
      edge('A', 'Two'),
      edge('B', 'Two'),
      edge('A', 'C'),
    ],
  };
  expect(ids(filterConnections(data, HIDE_ALL))).toEqual([
    'A',
    'B',
    'C',
    'Full',
    'Two',
  ]);
});

test('an empty page that only links out is not a leaf', () => {
  const data: Connections = {
    nodes: [node('A'), node('E', true)],
    edges: [edge('E', 'A')],
  };
  expect(ids(filterConnections(data, HIDE_ALL))).toEqual(['A', 'E']);
});

test('leaves are judged after journals and property links are hidden', () => {
  const data: Connections = {
    nodes: [
      node('A'),
      node('J', false, true),
      node('E', true),
      node('F', true),
    ],
    edges: [
      edge('A', 'E'),
      edge('J', 'E'),
      edge('A', 'F'),
      edge('A', 'F', true),
    ],
  };
  // With the journal hidden `E` has one link in, and `F`'s property link is
  // hidden too, so both are leaves; that leaves `A` an orphan.
  expect(ids(filterConnections(data, HIDE_ALL))).toEqual([]);
  const withJournals = filterConnections(data, {
    ...HIDE_ALL,
    journals: true,
  });
  expect(ids(withJournals)).toEqual(['A', 'E', 'J']);
});

test('hiding a leaf can leave its neighbour an orphan', () => {
  const data: Connections = {
    nodes: [node('A'), node('E', true)],
    edges: [edge('A', 'E')],
  };
  expect(ids(filterConnections(data, HIDE_ALL))).toEqual([]);
  expect(ids(filterConnections(data, { ...HIDE_ALL, orphans: true }))).toEqual([
    'A',
  ]);
});
