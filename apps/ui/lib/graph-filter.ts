import type { Connections } from './model';

/** What the graph view leaves out unless asked. */
export interface GraphShow {
  /** The lighter links a property makes to the entity that defines its key. */
  propertyLinks: boolean;
  journals: boolean;
  /** Pages with no content whose only link is one from another page. */
  emptyLeaves: boolean;
  /** Pages with no links in or out. */
  orphans: boolean;
}

export const HIDE_ALL: GraphShow = {
  propertyLinks: false,
  journals: false,
  emptyLeaves: false,
  orphans: false,
};

/**
 * What to draw of `data`. **Leaves and orphans are judged on what is left**
 * once journals and property links have been hidden: a page only a hidden
 * journal linked to, or only a property joined, has nothing drawn to it, so it
 * goes too. An empty leaf has no content and exactly one link, which comes in;
 * hiding it can leave its neighbour an orphan, but doesn't look for leaves
 * again.
 */
export function filterConnections(
  data: Connections,
  show: GraphShow,
): Connections {
  const shown = new Set(
    data.nodes.filter((n) => show.journals || !n.journal).map((n) => n.id),
  );
  let edges = data.edges.filter(
    (e) =>
      (show.propertyLinks || !e.weak) &&
      shown.has(e.source) &&
      shown.has(e.target),
  );
  if (!show.emptyLeaves) {
    const empty = new Set(data.nodes.filter((n) => n.empty).map((n) => n.id));
    const degree = new Map<string, number>();
    const inbound = new Set<string>();
    for (const { source, target } of edges) {
      degree.set(source, (degree.get(source) ?? 0) + 1);
      degree.set(target, (degree.get(target) ?? 0) + 1);
      inbound.add(target);
    }
    const leaf = (id: string) =>
      empty.has(id) && degree.get(id) === 1 && inbound.has(id);
    for (const id of shown) {
      if (leaf(id)) {
        shown.delete(id);
      }
    }
    edges = edges.filter((e) => shown.has(e.source) && shown.has(e.target));
  }
  const linked = new Set<string>();
  for (const { source, target } of edges) {
    linked.add(source);
    linked.add(target);
  }
  const nodes = data.nodes.filter(
    (n) => shown.has(n.id) && (show.orphans || linked.has(n.id)),
  );
  return { nodes, edges };
}
