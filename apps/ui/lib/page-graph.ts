import type { Connections, EntityId } from "./model";

/**
 * The part of the graph around one page: the page, its neighbours, and those
 * entities two steps away that touch two or more of those neighbours. A lone
 * second-degree entity is one more hanging off a neighbour, and the page would
 * drown in them; one that touches two neighbours is a link between them that
 * would otherwise be missing. Other second-degree entities don't count.
 *
 * Like the graph view's defaults, **property links and journals are left out**,
 * so a date's page doesn't make every page that mentions it a neighbour. The
 * page itself stays even if it is a journal. Direction doesn't matter.
 */
export function pageGraph(all: Connections, current: EntityId): Connections {
  const nodes = all.nodes.filter((n) => n.id === current || !n.journal);
  const known = new Set(nodes.map((n) => n.id));
  if (!known.has(current)) return { nodes: [], edges: [] };

  const edges = all.edges.filter(
    (e) => !e.weak && e.source !== e.target && known.has(e.source) && known.has(e.target),
  );
  const adjacent = new Map<EntityId, Set<EntityId>>();
  const link = (a: EntityId, b: EntityId) => {
    const set = adjacent.get(a) ?? new Set<EntityId>();
    set.add(b);
    adjacent.set(a, set);
  };
  for (const { source, target } of edges) {
    link(source, target);
    link(target, source);
  }

  const first = adjacent.get(current) ?? new Set<EntityId>();
  const second = new Set<EntityId>();
  for (const n of first) {
    for (const m of adjacent.get(n) ?? []) {
      if (m !== current && !first.has(m)) second.add(m);
    }
  }

  const kept = new Set<EntityId>([current, ...first]);
  for (const m of second) {
    let ties = 0;
    for (const x of adjacent.get(m) ?? []) if (first.has(x)) ties++;
    if (ties >= 2) kept.add(m);
  }

  return {
    nodes: nodes.filter((n) => kept.has(n.id)),
    edges: edges.filter((e) => kept.has(e.source) && kept.has(e.target)),
  };
}
